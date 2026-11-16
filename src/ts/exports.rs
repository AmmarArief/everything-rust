//! Exported functions of a TS/JS file, with their parameter and return types as written.

use oxc_allocator::Allocator;
use oxc_ast::ast::{
    BindingPattern, Declaration, Expression, FormalParameters, Statement, TSTypeAnnotation,
};
use oxc_parser::Parser;
use oxc_span::{GetSpan, SourceType};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Param {
    pub name: String,
    /// The TS type as written; `unknown` when there is no annotation.
    pub ts_type: String,
    pub optional: bool,
    pub rest: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportedFn {
    pub name: String,
    /// Source file, relative to the repository root.
    pub file: String,
    pub line: u32,
    pub params: Vec<Param>,
    /// The TS return type as written; `unknown` when there is no annotation.
    pub returns: String,
    pub is_async: bool,
}

/// Top-level `export function f()` and `export const f = () => ...` declarations.
pub fn exported_functions(path: &str, source: &str) -> Vec<ExportedFn> {
    let source_type = SourceType::from_path(path).unwrap_or_else(|_| SourceType::ts());
    let allocator = Allocator::default();
    let ret = Parser::new(&allocator, source, source_type).parse();
    let text = |span: oxc_span::Span| source[span.start as usize..span.end as usize].to_string();
    let annotation = |ann: Option<&TSTypeAnnotation>| {
        ann.map_or_else(|| "unknown".to_string(), |a| text(a.type_annotation.span()))
    };
    let line = |offset: u32| {
        u32::try_from(source[..offset as usize].matches('\n').count() + 1).unwrap_or(u32::MAX)
    };
    let params = |params: &FormalParameters| -> Vec<Param> {
        let mut out: Vec<Param> = params
            .items
            .iter()
            .enumerate()
            .map(|(i, p)| Param {
                name: binding_name(&p.pattern).unwrap_or_else(|| format!("arg{i}")),
                ts_type: annotation(p.type_annotation.as_deref()),
                optional: p.optional || p.initializer.is_some(),
                rest: false,
            })
            .collect();
        if let Some(rest) = &params.rest {
            out.push(Param {
                name: binding_name(&rest.rest.argument).unwrap_or_else(|| "rest".into()),
                ts_type: annotation(rest.type_annotation.as_deref()),
                optional: true,
                rest: true,
            });
        }
        out
    };

    let mut out = Vec::new();
    for statement in &ret.program.body {
        let declaration = match statement {
            Statement::ExportDeclaration(export) => Some(&export.declaration),
            _ => None,
        };
        match declaration {
            Some(Declaration::FunctionDeclaration(f)) => {
                if let Some(id) = &f.id {
                    out.push(ExportedFn {
                        name: id.name.as_str().to_string(),
                        file: path.to_string(),
                        line: line(f.span.start),
                        params: params(&f.params),
                        returns: annotation(f.return_type.as_deref()),
                        is_async: f.r#async,
                    });
                }
            }
            Some(Declaration::VariableDeclaration(vars)) => {
                for declarator in &vars.declarations {
                    let Some(name) = binding_name(&declarator.id) else {
                        continue;
                    };
                    let (fn_params, returns, is_async, span) = match &declarator.init {
                        Some(Expression::ArrowFunctionExpression(f)) => {
                            (&f.params, f.return_type.as_deref(), f.r#async, f.span)
                        }
                        Some(Expression::FunctionExpression(f)) => {
                            (&f.params, f.return_type.as_deref(), f.r#async, f.span)
                        }
                        _ => continue,
                    };
                    out.push(ExportedFn {
                        name,
                        file: path.to_string(),
                        line: line(span.start),
                        params: params(fn_params),
                        returns: annotation(returns),
                        is_async,
                    });
                }
            }
            _ => {}
        }
    }
    out
}

fn binding_name(pattern: &BindingPattern) -> Option<String> {
    match pattern {
        BindingPattern::BindingIdentifier(id) => Some(id.name.as_str().to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_function_and_arrow_exports() {
        let found = exported_functions(
            "a.ts",
            "export function score(query: string, text: string): number { return 1 }\n\
             export const words = (text: string, ...extra: string[]): string[] => []\n\
             export async function load(path?: string) {}\n\
             function hidden() {}\n\
             export const value = 1\n",
        );
        let names: Vec<&str> = found.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["score", "words", "load"]);
        assert_eq!(found[0].params[1].ts_type, "string");
        assert_eq!(found[0].returns, "number");
        assert_eq!(found[1].line, 2);
        assert!(found[1].params[1].rest);
        assert!(found[2].is_async && found[2].params[0].optional);
        assert_eq!(found[2].returns, "unknown");
    }
}
