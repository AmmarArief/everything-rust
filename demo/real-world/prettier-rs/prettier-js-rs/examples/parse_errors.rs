use std::collections::BTreeMap;

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_default();
    let mut counts: BTreeMap<String, (usize, String)> = BTreeMap::new();
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .collect();
    entries.sort();
    for path in entries {
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        let toml = std::fs::read_to_string(&path).unwrap_or_default();
        let Some(line) = toml.lines().find(|l| l.starts_with("args")) else {
            continue;
        };
        let Some(start) = line.find('[') else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&line[start..]) else {
            continue;
        };
        let argv: Vec<String> = v
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let Some(opts) = prettier_js_rs::Options::from_cli(&argv) else {
            continue;
        };
        let stdin = std::fs::read_to_string(path.with_extension("stdin")).unwrap_or_default();
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
        if !(ret.diagnostics.has_errors() || ret.diagnostics.has_warnings()) {
            continue;
        }
        let msg = ret
            .diagnostics
            .errors()
            .chain(ret.diagnostics.warnings())
            .next()
            .map(|d| d.to_string())
            .unwrap_or_default();
        let key: String = msg.clone();
        let status_ok = !toml.contains("status") || toml.contains("status.code = 0");
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let e = counts
            .entry(format!(
                "{} {}",
                if status_ok {
                    "PRETTIER-OK"
                } else {
                    "PRETTIER-ERR"
                },
                key
            ))
            .or_insert((0, name));
        e.0 += 1;
    }
    let mut v: Vec<_> = counts.into_iter().collect();
    v.sort_by_key(|x| std::cmp::Reverse(x.1.0));
    for (k, (n, ex)) in v.iter().take(200) {
        println!("{n:4} {k}  [{ex}]");
    }
}
