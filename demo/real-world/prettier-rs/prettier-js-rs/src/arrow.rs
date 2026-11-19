//! `src/language-js/print/arrow-function.js`.

use crate::value::Value;

use crate::doc::*;
use crate::function::should_print_params_without_parens;
use crate::node::*;
use crate::printer::{Args, Printer, R};
use crate::utils::*;

fn should_add_parens_if_not_break(n: &Value) -> bool {
    n.is("ConditionalExpression")
        && !starts_with_no_lookahead_token(n, &|x| x.is("ObjectExpression"))
}

pub fn print_arrow_function(p: &mut Printer<'_>, args: &Args) -> R {
    let print_as_chain = !args.expand_last_arg
        && p.node()
            .field("body")
            .is_some_and(|b| b.is("ArrowFunctionExpression"));
    let mut signatures: Vec<Doc> = Vec::new();
    let mut should_break_chain = false;
    let mut body_comments: Vec<Doc> = Vec::new();
    let (body_doc, function_body) = rec(
        p,
        args,
        print_as_chain,
        &mut signatures,
        &mut should_break_chain,
        &mut body_comments,
    )?;
    let should_put_body_on_same_line =
        !has_leading_own_line_comment(&p.comments, p.text, function_body)
            && (function_body.is("SequenceExpression")
                || may_break_after_short_prefix(function_body, p.text)
                || (!should_break_chain && should_add_parens_if_not_break(function_body)));
    let key = p.key();
    let parent = p.parent();
    let is_callee = key == Some("callee") && parent.is_some_and(is_call_like_expression);
    let chain_id = p.new_id("arrow-chain");
    let signatures_doc = print_signatures(p, args, signatures, should_break_chain);
    let mut should_break_signatures = false;
    let mut should_indent_signatures = false;
    let mut softline_in_indent = false;
    if print_as_chain && (is_callee || args.assignment_layout.is_some()) {
        should_indent_signatures = true;
        softline_in_indent = !p.has_any_comment(p.node());
        should_break_signatures = args.assignment_layout == Some("chain-tail-arrow-chain")
            || (is_callee && !should_put_body_on_same_line);
    }
    let body = print_body(
        p,
        args,
        body_doc,
        concat(body_comments),
        function_body,
        should_put_body_on_same_line,
    );
    Ok(group(concat(vec![
        group_with(
            if should_indent_signatures {
                indent(concat(vec![
                    if softline_in_indent {
                        softline()
                    } else {
                        empty()
                    },
                    signatures_doc,
                ]))
            } else {
                signatures_doc
            },
            should_break_signatures,
            Some(chain_id.clone()),
        ),
        s(" =>"),
        if print_as_chain {
            indent_if_break(body, Some(chain_id.clone()), false)
        } else {
            group(body)
        },
        if print_as_chain && is_callee {
            if_break_id(softline(), empty(), Some(chain_id))
        } else {
            empty()
        },
    ])))
}

fn rec<'a>(
    p: &mut Printer<'a>,
    args: &Args,
    print_as_chain: bool,
    signatures: &mut Vec<Doc>,
    should_break_chain: &mut bool,
    body_comments: &mut Vec<Doc>,
) -> R<(Doc, &'a Value)> {
    let node = p.node();
    let signature = print_signature(p, args)?;
    if signatures.is_empty() {
        signatures.push(signature);
    } else {
        let (leading, trailing) =
            crate::comments::print_comments_separately(&p.comments, node, p.text)
                .unwrap_or_default();
        signatures.push(concat(vec![concat(leading), signature]));
        body_comments.insert(0, concat(trailing));
    }
    if print_as_chain {
        *should_break_chain |= (node.field("returnType").is_some()
            && !function_parameters(node).is_empty())
            || node.field("typeParameters").is_some()
            || function_parameters(node)
                .iter()
                .any(|pa| !pa.is("Identifier"));
    }
    let body = node.field("body").unwrap_or(node);
    if !print_as_chain || !body.is("ArrowFunctionExpression") {
        let doc = p.print_args("body", args.clone())?;
        return Ok((doc, body));
    }
    p.call("body", |p| {
        rec(
            p,
            args,
            print_as_chain,
            signatures,
            should_break_chain,
            body_comments,
        )
    })
    .unwrap_or_else(|| crate::printer::bail("arrow body"))
}

fn print_signature(p: &mut Printer<'_>, args: &Args) -> R {
    let node = p.node();
    let mut parts = Vec::new();
    if node.flag("async") {
        parts.push(s("async "));
    }
    if should_print_params_without_parens(p) {
        parts.push(p.print_index("params", 0)?);
    } else {
        let expand = args.expand_last_arg || args.expand_first_arg;
        let mut return_type = crate::typescript::print_return_type(p)?;
        if expand {
            if will_break(&return_type) {
                return crate::printer::bail(crate::call::ARG_EXPANSION_BAILOUT);
            }
            return_type = group(remove_lines(&return_type));
        }
        parts.push(group(concat(vec![
            crate::function::print_function_parameters_ext(p, expand, true)?,
            return_type,
        ])));
    }
    let text = p.text;
    let dangling = crate::comments::print_dangling_comments(
        &p.comments,
        node,
        text,
        false,
        None,
        Some(&|c: &crate::comments::Comment| {
            crate::text::next_non_space_non_comment_index(text, c.end)
                .is_some_and(|i| text.get(i..i + 2) == Some("=>"))
        }),
    );
    if !is_empty(&dangling) {
        parts.push(s(" "));
        parts.push(dangling);
    }
    Ok(concat(parts))
}

fn may_break_after_short_prefix(body: &Value, text: &str) -> bool {
    body.is("ArrayExpression")
        || body.is("ObjectExpression")
        || body.is("ArrowFunctionExpression")
        || body.is("DoExpression")
        || body.is("BlockStatement")
        || is_jsx_element(body)
        || is_template_on_its_own_line(body, text)
}

fn print_signatures(p: &Printer<'_>, args: &Args, signatures: Vec<Doc>, should_break: bool) -> Doc {
    if signatures.len() == 1 {
        return signatures.into_iter().next().unwrap_or_else(empty);
    }
    let key = p.key();
    let parent = p.parent();
    let arrow_sep = concat(vec![s(" =>"), line()]);
    if (key != Some("callee") && parent.is_some_and(is_call_like_expression))
        || parent.is_some_and(is_binaryish)
    {
        let mut it = signatures.into_iter();
        let first = it.next().unwrap_or_else(empty);
        return group_with(
            concat(vec![
                first,
                s(" =>"),
                indent(concat(vec![line(), join(&arrow_sep, it.collect())])),
            ]),
            should_break,
            None,
        );
    }
    if (key == Some("callee") && parent.is_some_and(is_call_like_expression))
        || args.assignment_layout.is_some()
    {
        return group_with(join(&arrow_sep, signatures), should_break, None);
    }
    group_with(indent(join(&arrow_sep, signatures)), should_break, None)
}

fn print_body(
    p: &Printer<'_>,
    args: &Args,
    body: Doc,
    body_comments: Doc,
    function_body: &Value,
    same_line: bool,
) -> Doc {
    let parent = p.parent();
    let trailing_comma = if args.expand_last_arg && p.opts.should_print_comma(false) {
        if_break(s(","), empty())
    } else {
        empty()
    };
    let trailing_space = if (args.expand_last_arg
        || parent.is_some_and(|pa| pa.is("JSXExpressionContainer")))
        && !p.has_any_comment(p.node())
    {
        softline()
    } else {
        empty()
    };
    if same_line && should_add_parens_if_not_break(function_body) {
        return concat(vec![
            s(" "),
            group(concat(vec![
                if_break(empty(), s("(")),
                indent(concat(vec![softline(), body])),
                if_break(empty(), s(")")),
                trailing_comma,
                trailing_space,
            ])),
            body_comments,
        ]);
    }
    if same_line {
        concat(vec![s(" "), body, body_comments])
    } else {
        concat(vec![
            indent(concat(vec![line(), body, body_comments])),
            trailing_comma,
            trailing_space,
        ])
    }
}
