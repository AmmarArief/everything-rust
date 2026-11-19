use std::collections::{BTreeMap, BTreeSet};

fn walk(v: &serde_json::Value, out: &mut BTreeSet<&'static str>) {
    match v {
        serde_json::Value::Array(a) => a.iter().for_each(|x| walk(x, out)),
        serde_json::Value::Object(o) => {
            if let Some(t) = o.get("type").and_then(|t| t.as_str()) {
                let f = match t {
                    "ConditionalExpression" => None,
                    "TemplateLiteral" => None,
                    "TaggedTemplateExpression" => None,
                    "ClassDeclaration" | "ClassExpression" => None,
                    "Decorator" => Some("decorator"),
                    _ if t.starts_with("JSX") => Some("jsx"),
                    _ if t.starts_with("TS") => Some("ts"),
                    _ => None,
                };
                if let Some(f) = f {
                    out.insert(f);
                }
            }
            for (k, x) in o {
                if matches!(
                    k.as_str(),
                    "typeAnnotation" | "returnType" | "typeParameters" | "typeArguments"
                ) && !x.is_null()
                {
                    out.insert("ts");
                }
                walk(x, out);
            }
        }
        _ => {}
    }
}

fn main() {
    let dir = std::env::args().nth(1).unwrap();
    let mut combos: BTreeMap<String, usize> = BTreeMap::new();
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .collect();
    entries.sort();
    for path in entries {
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        let toml = std::fs::read_to_string(&path).unwrap();
        let Some(line) = toml.lines().find(|l| l.starts_with("args")) else {
            continue;
        };
        let v: serde_json::Value = serde_json::from_str(&line[line.find('[').unwrap()..]).unwrap();
        let argv: Vec<String> = v
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|x| x.as_str().map(String::from))
            .collect();
        let Some(opts) = prettier_js_rs::Options::from_cli(&argv) else {
            *combos.entry("cli".into()).or_default() += 1;
            continue;
        };
        let stdin = std::fs::read_to_string(path.with_extension("stdin")).unwrap_or_default();
        if prettier_js_rs::format(&stdin, &opts).is_ok() {
            continue;
        }
        let lower = opts.filepath.to_ascii_lowercase();
        let mut st = oxc_span::SourceType::from_path(&opts.filepath)
            .unwrap_or_else(|_| oxc_span::SourceType::mjs());
        if lower.ends_with(".js") || lower.ends_with(".mjs") || lower.ends_with(".cjs") {
            st = st.with_jsx(true);
        }
        st = st.with_module(!(lower.ends_with(".cjs") || lower.ends_with(".cts")));
        let alloc = oxc_allocator::Allocator::default();
        let ret = oxc_parser::Parser::new(&alloc, &stdin, st)
            .with_options(oxc_parser::ParseOptions {
                preserve_parens: true,
                ..Default::default()
            })
            .parse();
        if ret.diagnostics.has_errors() || ret.diagnostics.has_warnings() {
            *combos.entry("parse".into()).or_default() += 1;
            continue;
        }
        let mut set = BTreeSet::new();
        if !ret.program.comments.is_empty() {
            set.insert("comments");
        }
        if stdin.contains('\r') {
            set.insert("cr");
        }
        if stdin.starts_with('\u{FEFF}') {
            set.insert("bom");
        }
        if lower.ends_with(".d.ts") {
            set.insert("dts");
        }
        let json = ret.program.to_estree_json(st.is_typescript(), false);
        let mut de = serde_json::Deserializer::from_str(&json);
        de.disable_recursion_limit();
        let ast: serde_json::Value = serde::Deserialize::deserialize(&mut de).unwrap();
        walk(&ast, &mut set);
        if set.is_empty() {
            set.insert("other");
        }
        let key = set.into_iter().collect::<Vec<_>>().join("+");
        *combos.entry(key).or_default() += 1;
    }
    let mut v: Vec<_> = combos.into_iter().collect();
    v.sort_by_key(|x| std::cmp::Reverse(x.1));
    for (k, n) in v.iter().take(45) {
        println!("{n:5} {k}");
    }
}
