//! `src/language-js/print/class.js`, without decorators, comments and TypeScript heritage.

use crate::value::Value;

use crate::doc::*;
use crate::node::*;
use crate::printer::{Printer, R};

fn is_class_property(n: &Value) -> bool {
    matches!(
        n.ty(),
        "ClassProperty"
            | "PropertyDefinition"
            | "ClassPrivateProperty"
            | "ClassAccessorProperty"
            | "AccessorProperty"
            | "TSAbstractPropertyDefinition"
            | "TSAbstractAccessorProperty"
    )
}

fn has_multiple_heritage(node: &Value) -> bool {
    let count = ["extends", "mixins", "implements"]
        .iter()
        .map(|k| node.list(k).len())
        .sum::<usize>()
        + usize::from(node.field("superClass").is_some());
    count > 1
}

fn should_indent_only_heritage_clauses(p: &Printer<'_>, node: &Value) -> bool {
    node.field("typeParameters").is_some_and(|tp| {
        !p.has_comment(
            tp,
            crate::comments::Flags::TRAILING.with(crate::comments::Flags::LINE),
        )
    }) && !has_multiple_heritage(node)
}

fn print_heritage_clauses(p: &mut Printer<'_>, list_name: &'static str) -> R {
    let node = p.node();
    if node.list(list_name).is_empty() {
        return Ok(empty());
    }
    let leading = p.print_dangling(node, false, Some(list_name));
    let first = if should_indent_only_heritage_clauses(p, node) {
        let id = node
            .field("typeParameters")
            .map(|tp| p.group_id_for("typeParameters", tp));
        if_break_id(s(" "), line(), id)
    } else {
        line()
    };
    let has_leading = !is_empty(&leading);
    let printed = p.print_all(list_name)?;
    Ok(concat(vec![
        first,
        leading,
        if has_leading { hardline() } else { empty() },
        s(list_name),
        group(indent(concat(vec![
            line(),
            join(&concat(vec![s(","), line()]), printed),
        ]))),
    ]))
}

pub fn print_class(p: &mut Printer<'_>) -> R {
    let node = p.node();
    let mut parts = vec![
        crate::typescript::print_declare_token(p),
        crate::typescript::print_abstract_token(p),
        s("class"),
    ];
    let group_mode = node
        .field("id")
        .is_some_and(|id| p.has_comment(id, crate::comments::Flags::TRAILING))
        || node
            .field("typeParameters")
            .is_some_and(|tp| p.has_comment(tp, crate::comments::Flags::TRAILING))
        || node
            .field("superClass")
            .is_some_and(|sc| p.has_any_comment(sc))
        || !node.list("extends").is_empty()
        || !node.list("mixins").is_empty()
        || !node.list("implements").is_empty();
    let mut parts_group = Vec::new();
    let mut extends_parts = Vec::new();
    if node.field("id").is_some() {
        parts_group.push(s(" "));
        parts_group.push(p.print("id")?);
    }
    parts_group.push(p.print("typeParameters")?);
    if let Some(super_class) = node.field("superClass") {
        let printed = p.print("superClass")?;
        let printed = if p.parent().is_some_and(|pa| pa.is("AssignmentExpression")) {
            group(if_break(
                concat(vec![
                    s("("),
                    indent(concat(vec![softline(), printed.clone()])),
                    softline(),
                    s(")"),
                ]),
                printed,
            ))
        } else {
            printed
        };
        let type_args_key = if node.field("superTypeArguments").is_some() {
            "superTypeArguments"
        } else {
            "superTypeParameters"
        };
        let type_args = p.print(type_args_key)?;
        let with_comments = concat(vec![
            s("extends "),
            crate::comments::print_comments(
                &p.comments,
                super_class,
                concat(vec![printed, type_args]),
                p.text,
            ),
        ]);
        if group_mode {
            extends_parts.push(line());
            extends_parts.push(group(with_comments));
        } else {
            extends_parts.push(s(" "));
            extends_parts.push(with_comments);
        }
    } else {
        extends_parts.push(print_heritage_clauses(p, "extends")?);
    }
    extends_parts.push(print_heritage_clauses(p, "mixins")?);
    extends_parts.push(print_heritage_clauses(p, "implements")?);
    let heritage_id = if group_mode {
        let id = p.group_id_for("heritageGroup", node);
        let printed = if should_indent_only_heritage_clauses(p, node) {
            parts_group.push(indent(concat(extends_parts)));
            concat(parts_group)
        } else {
            parts_group.push(concat(extends_parts));
            indent(concat(parts_group))
        };
        parts.push(group_with(printed, false, Some(id.clone())));
        Some(id)
    } else {
        parts.extend(parts_group);
        parts.extend(extends_parts);
        None
    };
    let body_non_empty = node
        .field("body")
        .is_some_and(|b| !b.list("body").is_empty());
    match heritage_id {
        Some(id) if body_non_empty => parts.push(if_break_id(hardline(), s(" "), Some(id))),
        _ => parts.push(s(" ")),
    }
    parts.push(p.print("body")?);
    Ok(concat(parts))
}

pub fn print_class_method(p: &mut Printer<'_>) -> R {
    let node = p.node();
    let mut parts = Vec::new();
    if !node.list("decorators").is_empty() {
        parts.push(print_class_member_decorators(p)?);
    }
    parts.push(crate::typescript::print_accessibility_token(node));
    if node.flag("static") {
        parts.push(s("static "));
    }
    parts.push(crate::typescript::print_abstract_token(p));
    if node.flag("override") {
        parts.push(s("override "));
    }
    parts.push(crate::function::print_method(p)?);
    Ok(concat(parts))
}

pub fn print_class_property(p: &mut Printer<'_>) -> R {
    let node = p.node();
    let mut parts = Vec::new();
    if !node.list("decorators").is_empty() {
        parts.push(print_class_member_decorators(p)?);
    }
    parts.push(crate::typescript::print_declare_token(p));
    parts.push(crate::typescript::print_accessibility_token(node));
    if node.flag("static") {
        parts.push(s("static "));
    }
    parts.push(crate::typescript::print_abstract_token(p));
    if node.flag("override") {
        parts.push(s("override "));
    }
    if node.flag("readonly") {
        parts.push(s("readonly "));
    }
    if matches!(
        node.ty(),
        "ClassAccessorProperty" | "AccessorProperty" | "TSAbstractAccessorProperty"
    ) {
        parts.push(s("accessor "));
    }
    parts.push(crate::property::print_property_key(p)?);
    parts.push(crate::member::print_optional_token(p));
    parts.push(crate::typescript::print_definite_token(p));
    parts.push(crate::typescript::print_type_annotation_property(
        p,
        "typeAnnotation",
    )?);
    let is_abstract = matches!(
        node.ty(),
        "TSAbstractPropertyDefinition" | "TSAbstractAccessorProperty"
    );
    let semi = if p.opts.semi { ";" } else { "" };
    let printed = crate::assignment::print_assignment(
        p,
        concat(parts),
        s(" ="),
        if is_abstract { None } else { Some("value") },
    )?;
    Ok(concat(vec![printed, s(semi)]))
}

pub fn print_class_body(p: &mut Printer<'_>) -> R {
    let node = p.node();
    let body = node.list("body");
    let mut parts = Vec::new();
    let semi = p.opts.semi;
    let text = p.text;
    p.each("body", |p, i| {
        let member = &body[i];
        parts.push(p.print_current(Default::default())?);
        if !semi
            && is_class_property(member)
            && should_print_semicolon_after_class_property(member, body.get(i + 1))
        {
            parts.push(s(";"));
        }
        if i + 1 < body.len() {
            parts.push(hardline());
            if crate::text::is_next_line_empty(text, member.end()) {
                parts.push(hardline());
            }
        }
        Ok(())
    })?;
    if p.has_comment(node, crate::comments::Flags::DANGLING) {
        parts.push(p.print_dangling(node, false, None));
    }
    Ok(concat(vec![
        s("{"),
        if parts.is_empty() {
            empty()
        } else {
            concat(vec![
                indent(concat(vec![hardline(), concat(parts)])),
                hardline(),
            ])
        },
        s("}"),
    ]))
}

fn should_print_semicolon_after_class_property(node: &Value, next: Option<&Value>) -> bool {
    let key = node.field("key");
    if !node.flag("computed")
        && key.is_some_and(|k| {
            k.is("Identifier") && matches!(k.str_of("name"), Some("static" | "get" | "set"))
        })
        && node.field("value").is_none()
        && node.field("typeAnnotation").is_none()
    {
        return true;
    }
    let Some(next) = next else {
        return false;
    };
    if next.flag("static") || next.field("accessibility").is_some() || next.flag("readonly") {
        return false;
    }
    if !next.flag("computed")
        && matches!(
            next.field("key").and_then(|k| k.str_of("name")),
            Some("in" | "instanceof")
        )
    {
        return true;
    }
    if is_class_property(next)
        && next.field("variance").is_some()
        && !next.flag("static")
        && !next.flag("declare")
    {
        return true;
    }
    match next.ty() {
        "ClassProperty" | "PropertyDefinition" | "TSAbstractPropertyDefinition" => {
            next.flag("computed")
        }
        "MethodDefinition"
        | "TSAbstractMethodDefinition"
        | "ClassMethod"
        | "ClassPrivateMethod" => {
            let value = next.field("value").unwrap_or(next);
            let kind = next.str_of("kind");
            if value.flag("async") || kind == Some("get") || kind == Some("set") {
                return false;
            }
            next.flag("computed") || value.flag("generator")
        }
        "TSIndexSignature" => true,
        _ => false,
    }
}

fn has_newline_between_or_after_decorators(p: &Printer<'_>, node: &Value) -> bool {
    node.list("decorators")
        .iter()
        .any(|d| crate::text::has_newline(p.text, d.end(), false))
}

pub fn has_decorators_before_export(node: &Value) -> bool {
    if !matches!(
        node.ty(),
        "ExportDefaultDeclaration" | "ExportNamedDeclaration" | "DeclareExportDeclaration"
    ) {
        return false;
    }
    node.field("declaration")
        .and_then(|d| d.list("decorators").first())
        .is_some_and(|first| node.start() == first.start())
}

pub fn print_class_member_decorators(p: &mut Printer<'_>) -> R {
    let node = p.node();
    let printed = p.print_all("decorators")?;
    Ok(group(concat(vec![
        join(&line(), printed),
        if has_newline_between_or_after_decorators(p, node) {
            hardline()
        } else {
            line()
        },
    ])))
}

pub fn print_decorators_before_export(p: &mut Printer<'_>) -> R {
    let node = p.node();
    if !has_decorators_before_export(node) {
        return Ok(empty());
    }
    let printed = p
        .call("declaration", |p| p.print_all("decorators"))
        .unwrap_or(Ok(Vec::new()))?;
    Ok(concat(vec![join(&hardline(), printed), hardline()]))
}

pub fn print_decorators(p: &mut Printer<'_>) -> R {
    let node = p.node();
    if node.list("decorators").is_empty()
        || p.parent().is_some_and(has_decorators_before_export)
        || p.is_ignored()
    {
        return Ok(empty());
    }
    let should_break = node.is("ClassExpression")
        || node.is("ClassDeclaration")
        || has_newline_between_or_after_decorators(p, node);
    let first = if p.key() == Some("declaration")
        && p.parent().is_some_and(|pa| {
            matches!(
                pa.ty(),
                "ExportDefaultDeclaration"
                    | "ExportNamedDeclaration"
                    | "DeclareExportDeclaration"
                    | "ExportAllDeclaration"
                    | "DeclareExportAllDeclaration"
            )
        }) {
        hardline()
    } else if should_break {
        break_parent()
    } else {
        empty()
    };
    let printed = p.print_all("decorators")?;
    Ok(concat(vec![first, join(&line(), printed), line()]))
}
