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
    #[serde(default)]
    pub is_generator: bool,
    /// Names of the function's own type parameters (`T` in `f<T>(x: T)`).
    #[serde(default)]
    pub type_params: Vec<String>,
    /// The body checks `instanceof`: class identity, which does not survive a bridge.
    #[serde(default)]
    pub uses_instanceof: bool,
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
                ts_type: match (&p.type_annotation, p.initializer.as_deref()) {
                    (Some(a), _) => annotation(Some(a)),
                    // `namespaced = true`: the type comes from the default value.
                    (None, Some(Expression::BooleanLiteral(_))) => "boolean".into(),
                    (None, Some(Expression::NumericLiteral(_))) => "number".into(),
                    (None, Some(Expression::StringLiteral(_) | Expression::TemplateLiteral(_))) => {
                        "string".into()
                    }
                    (None, _) => "unknown".into(),
                },
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
            // Overload signatures have no body: the implementation signature follows them.
            Some(Declaration::FunctionDeclaration(f)) if f.body.is_some() => {
                if let Some(id) = &f.id {
                    out.push(ExportedFn {
                        name: id.name.as_str().to_string(),
                        file: path.to_string(),
                        line: line(f.span.start),
                        params: params(&f.params),
                        returns: annotation(f.return_type.as_deref()),
                        is_async: f.r#async,
                        is_generator: f.generator,
                        type_params: f
                            .type_parameters
                            .as_ref()
                            .map(|tp| {
                                tp.params
                                    .iter()
                                    .map(|p| p.name.name.as_str().to_string())
                                    .collect()
                            })
                            .unwrap_or_default(),
                        uses_instanceof: uses_instanceof(&text(f.span)),
                    });
                }
            }
            Some(Declaration::VariableDeclaration(vars)) => {
                for declarator in &vars.declarations {
                    let Some(name) = binding_name(&declarator.id) else {
                        continue;
                    };
                    let type_names =
                        |tp: Option<&oxc_ast::ast::TSTypeParameterDeclaration>| -> Vec<String> {
                            tp.map(|tp| {
                                tp.params
                                    .iter()
                                    .map(|p| p.name.name.as_str().to_string())
                                    .collect()
                            })
                            .unwrap_or_default()
                        };
                    let (fn_params, returns, is_async, is_generator, span, type_params) =
                        match &declarator.init {
                            Some(Expression::ArrowFunctionExpression(f)) => (
                                &f.params,
                                f.return_type.as_deref(),
                                f.r#async,
                                false,
                                f.span,
                                type_names(f.type_parameters.as_deref()),
                            ),
                            Some(Expression::FunctionExpression(f)) => (
                                &f.params,
                                f.return_type.as_deref(),
                                f.r#async,
                                f.generator,
                                f.span,
                                type_names(f.type_parameters.as_deref()),
                            ),
                            _ => continue,
                        };
                    out.push(ExportedFn {
                        name,
                        file: path.to_string(),
                        line: line(span.start),
                        params: params(fn_params),
                        returns: annotation(returns),
                        is_async,
                        is_generator,
                        type_params,
                        uses_instanceof: uses_instanceof(&text(span)),
                    });
                }
            }
            _ => {}
        }
    }
    out
}

fn uses_instanceof(code: &str) -> bool {
    code.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'))
        .any(|word| word == "instanceof")
}

/// A top-level interface or type alias.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalType {
    pub name: String,
    /// The whole interface, or the right-hand side of a type alias.
    pub text: String,
    /// A type alias without type parameters: its uses can be replaced by `text`.
    pub alias: bool,
}

/// Interfaces and type aliases declared at the top level of a file.
pub fn local_types(path: &str, source: &str) -> Vec<LocalType> {
    let source_type = SourceType::from_path(path).unwrap_or_else(|_| SourceType::ts());
    let allocator = Allocator::default();
    let ret = Parser::new(&allocator, source, source_type).parse();
    let text = |span: oxc_span::Span| source[span.start as usize..span.end as usize].to_string();
    let mut out = Vec::new();
    for statement in &ret.program.body {
        let declaration = match statement {
            Statement::ExportDeclaration(export) => &export.declaration,
            Statement::TSInterfaceDeclaration(_) | Statement::TSTypeAliasDeclaration(_) => {
                match statement.as_declaration() {
                    Some(d) => d,
                    None => continue,
                }
            }
            _ => continue,
        };
        match declaration {
            Declaration::TSInterfaceDeclaration(d) => out.push(LocalType {
                name: d.id.name.as_str().to_string(),
                text: text(d.span),
                alias: false,
            }),
            Declaration::TSTypeAliasDeclaration(d) => out.push(LocalType {
                name: d.id.name.as_str().to_string(),
                text: text(d.type_annotation.span()),
                alias: d.type_parameters.is_none(),
            }),
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
             export async function load(path?: string, strict = false) {}\n\
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
        assert_eq!(
            (
                found[2].params[1].ts_type.as_str(),
                found[2].params[1].optional
            ),
            ("boolean", true)
        );
        assert_eq!(found[2].returns, "unknown");
    }

    #[test]
    fn notices_instanceof() {
        let found = exported_functions(
            "a.ts",
            "export function isErr(e: unknown): boolean { return e instanceof Error }\n\
             export const plain = (instanceofCount: number): number => instanceofCount\n",
        );
        assert!(found[0].uses_instanceof);
        assert!(!found[1].uses_instanceof);
    }

    #[test]
    fn reads_local_types() {
        let types = local_types(
            "a.ts",
            "export interface LineIndex { readonly line: (n: number) => string }\ntype Id = string\n",
        );
        let names: Vec<&str> = types.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["LineIndex", "Id"]);
        assert!(types[0].text.contains("=>") && !types[0].alias);
        assert_eq!((types[1].text.as_str(), types[1].alias), ("string", true));
    }
}
