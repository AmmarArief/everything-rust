//! `src/language-js/print/member.js` and `printOptionalToken` (`misc.js`).

use crate::value::Value;

use crate::doc::*;
use crate::node::*;
use crate::printer::{Printer, R};

pub fn print_optional_token(p: &Printer<'_>) -> Doc {
    let node = p.node();
    if !node.flag("optional") {
        return empty();
    }
    if node.is("Identifier")
        && p.parent()
            .is_some_and(|pa| pa.field("key").is_some_and(|k| k.same(node)))
    {
        return empty();
    }
    if is_call_expression(node) || (is_member_expression(node) && node.flag("computed")) {
        return s("?.");
    }
    s("?")
}

fn is_call_expression_with_arguments(n: &Value) -> bool {
    let n = if n.is("ChainExpression") || n.is("TSNonNullExpression") {
        n.field("expression").unwrap_or(n)
    } else {
        n
    };
    is_call_expression(n) && !crate::utils::call_arguments(n).is_empty()
}

pub fn print_member_expression(p: &mut Printer<'_>) -> R {
    let object_doc = p.print("object")?;
    let lookup = print_member_lookup(p)?;
    let node = p.node();
    let first_non_member_parent =
        p.find_ancestor(|n| !(is_member_expression(n) || n.is("TSNonNullExpression")));
    let first_non_chain_parent =
        p.find_ancestor(|n| !(n.is("ChainExpression") || n.is("TSNonNullExpression")));
    let object = node.field("object");
    let object_label = label_of(&object_doc).map(str::to_string);
    let should_inline = first_non_member_parent.is_some_and(|f| {
        f.is("NewExpression")
            || f.is("BindExpression")
            || (f.is("AssignmentExpression")
                && !f.field("left").is_some_and(|l| l.is("Identifier")))
    }) || node.flag("computed")
        || (object.is_some_and(|o| o.is("Identifier"))
            && node.field("property").is_some_and(|pr| pr.is("Identifier"))
            && !first_non_chain_parent.is_some_and(is_member_expression))
        || (first_non_chain_parent
            .is_some_and(|f| f.is("AssignmentExpression") || f.is("VariableDeclarator"))
            && (object.is_some_and(is_call_expression_with_arguments)
                || object_label.as_deref() == Some("member-chain")));
    let printed = concat(vec![
        object_doc,
        if should_inline {
            lookup
        } else {
            group(indent(concat(vec![softline(), lookup])))
        },
    ]);
    Ok(match object_label {
        Some(l) => label(&l, printed),
        None => printed,
    })
}

pub fn print_member_lookup(p: &mut Printer<'_>) -> R {
    let property = p.print("property")?;
    let node = p.node();
    let optional = print_optional_token(p);
    if !node.flag("computed") {
        return Ok(concat(vec![optional, s("."), property]));
    }
    if node.field("property").is_none_or(is_numeric_literal) {
        return Ok(concat(vec![optional, s("["), property, s("]")]));
    }
    Ok(group(concat(vec![
        optional,
        s("["),
        indent(concat(vec![softline(), property])),
        softline(),
        s("]"),
    ])))
}
