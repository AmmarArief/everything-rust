//! `src/utils/ignore.js`, `src/config/resolve-config.js`, `src/config/editorconfig/`,
//! `src/config/prettier-config/` and the `editorconfig` package's parser, as Prettier's CLI
//! applies them to `--stdin-filepath`.

use std::path::{Component, Path, PathBuf};

use serde_json::{Map, Value};

const CONFIG_FILES: [&str; 20] = [
    "package.json",
    "package.yaml",
    ".prettierrc",
    ".prettierrc.json",
    ".prettierrc.yaml",
    ".prettierrc.yml",
    ".prettierrc.json5",
    ".prettierrc.js",
    ".prettierrc.ts",
    ".prettierrc.mjs",
    ".prettierrc.mts",
    ".prettierrc.cjs",
    ".prettierrc.cts",
    "prettier.config.js",
    "prettier.config.ts",
    "prettier.config.mjs",
    "prettier.config.mts",
    "prettier.config.cjs",
    "prettier.config.cts",
    ".prettierrc.toml",
];

pub fn resolve(filepath: &str) -> Option<Vec<String>> {
    let cwd = std::env::current_dir().ok()?;
    let file = normalize(&cwd.join(filepath));
    if is_ignored(&cwd, &file)? {
        return None;
    }
    let dir = file.parent()?;
    let mut options = editorconfig(dir, &file)?;
    if let Some(config) = prettier_config(dir, &file)? {
        for (k, v) in config {
            options.insert(k, v);
        }
    }
    to_args(&options)
}

fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn relative(from: &Path, to: &Path) -> Option<String> {
    let rel = to.strip_prefix(from).ok()?;
    let parts: Option<Vec<&str>> = rel.components().map(|c| c.as_os_str().to_str()).collect();
    Some(parts?.join("/"))
}

fn is_ignored(cwd: &Path, file: &Path) -> Option<bool> {
    let rel = relative(cwd, file)?;
    if rel.is_empty() {
        return None;
    }
    for name in [".gitignore", ".prettierignore"] {
        let content = match std::fs::read(cwd.join(name)) {
            Ok(bytes) => String::from_utf8(bytes).ok()?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(_) => return None,
        };
        if content.trim().is_empty() {
            if rel.split('/').any(|part| part == "node_modules") {
                return Some(true);
            }
            continue;
        }
        let content = format!("{content}\nnode_modules");
        let mut builder = ignore::gitignore::GitignoreBuilder::new(cwd);
        for line in content.lines() {
            if line.contains('\\') || line.contains('[') {
                return None;
            }
            builder.add_line(None, line).ok()?;
        }
        let matcher = builder.build().ok()?;
        if matcher
            .matched_path_or_any_parents(cwd.join(&rel), false)
            .is_ignore()
        {
            return Some(true);
        }
    }
    Some(false)
}

fn glob_to_regex(glob: &str, any_depth_double_star: bool) -> Option<String> {
    let chars: Vec<char> = glob.chars().collect();
    let mut out = String::from("^");
    let mut i = 0;
    let mut in_brace = false;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '*' if chars.get(i + 1) == Some(&'*') => {
                let at_start = i == 0 || chars[i - 1] == '/';
                if chars.get(i + 2) == Some(&'/') && at_start {
                    out.push_str("(?:.*/)?");
                    i += 3;
                    continue;
                }
                if i + 2 == chars.len() && at_start {
                    out.push_str(".*");
                    i += 2;
                    continue;
                }
                if !any_depth_double_star {
                    return None;
                }
                out.push_str(".*");
                i += 2;
                continue;
            }
            '*' => out.push_str("[^/]*"),
            '?' => out.push_str("[^/]"),
            '{' if !in_brace => {
                let close = chars[i..].iter().position(|&x| x == '}')?;
                let inner: String = chars[i + 1..i + close].iter().collect();
                if inner.contains("..") || inner.contains('{') || !inner.contains(',') {
                    return None;
                }
                in_brace = true;
                out.push_str("(?:");
            }
            ',' if in_brace => out.push('|'),
            '}' if in_brace => {
                in_brace = false;
                out.push(')');
            }
            '[' | ']' | '(' | ')' | '!' | '+' | '@' | '\\' | '{' | '}' => return None,
            other => out.push_str(&regex::escape(&other.to_string())),
        }
        i += 1;
    }
    if in_brace {
        return None;
    }
    out.push('$');
    Some(out)
}

fn glob_matches(glob: &str, text: &str, any_depth_double_star: bool) -> Option<bool> {
    let re = regex::Regex::new(&glob_to_regex(glob, any_depth_double_star)?).ok()?;
    Some(re.is_match(text))
}

fn find_project_root(dir: &Path) -> Option<PathBuf> {
    let mut d = Some(dir);
    while let Some(cur) = d {
        for name in [".git", ".hg"] {
            if std::fs::symlink_metadata(cur.join(name)).is_ok_and(|m| m.is_dir()) {
                return Some(cur.to_path_buf());
            }
        }
        d = cur.parent();
    }
    None
}

type Section = (Option<String>, Vec<(String, String)>);

fn parse_ini(text: &str) -> Vec<Section> {
    let mut out: Vec<Section> = vec![(None, Vec::new())];
    if text.is_empty() {
        return out;
    }
    let comment = regex::Regex::new(r"^\s*[#;].*$").ok();
    let param = regex::Regex::new(r"^\s*([\w\.\-_]+)\s*[=:]\s*(.*?)\s*([#;].*)?$").ok();
    let section = regex::Regex::new(r"^\s*\[(([^#;]|\\#|\\;)+)\]\s*([#;].*)?$").ok();
    let (Some(comment), Some(param), Some(section)) = (comment, param, section) else {
        return Vec::new();
    };
    for line in text.split("\r\n").flat_map(|l| l.split(['\r', '\n'])) {
        if comment.is_match(line) {
            continue;
        }
        if let Some(m) = param.captures(line) {
            if let Some(last) = out.last_mut() {
                let key = m[1].to_string();
                let value = m.get(2).map_or("", |v| v.as_str()).to_string();
                last.1.retain(|(k, _)| *k != key);
                last.1.push((key, value));
            }
        } else if let Some(m) = section.captures(line) {
            out.push((Some(m[1].to_string()), Vec::new()));
        }
    }
    out
}

fn editorconfig_value(key: &str, value: &str) -> Value {
    let known = matches!(
        key,
        "end_of_line"
            | "indent_style"
            | "indent_size"
            | "insert_final_newline"
            | "trim_trailing_whitespace"
            | "charset"
    );
    let lowered = if known {
        value.to_lowercase()
    } else {
        value.to_string()
    };
    match serde_json::from_str::<Value>(value) {
        Ok(v) => v,
        Err(_) => Value::String(lowered),
    }
}

fn editorconfig(dir: &Path, file: &Path) -> Option<Map<String, Value>> {
    let root = find_project_root(dir).unwrap_or_else(|| PathBuf::from("/"));
    let mut files: Vec<(PathBuf, String)> = Vec::new();
    let mut d = Some(dir);
    while let Some(cur) = d {
        let path = cur.join(".editorconfig");
        let text = match std::fs::read(&path) {
            Ok(bytes) => String::from_utf8(bytes).ok()?,
            Err(_) => String::new(),
        };
        files.push((cur.to_path_buf(), text));
        if cur == root {
            break;
        }
        d = cur.parent();
    }
    let mut configs: Vec<(PathBuf, Vec<Section>)> = Vec::new();
    for (cur, text) in files {
        let sections = parse_ini(&text);
        let is_root = sections
            .first()
            .and_then(|s| s.1.iter().rev().find(|(k, _)| k == "root"))
            .is_some_and(|(_, v)| v.to_lowercase() == "true");
        configs.push((cur, sections));
        if is_root {
            break;
        }
    }
    let file_str = file.to_str()?;
    let mut matches: Map<String, Value> = Map::new();
    for (cur, sections) in configs.iter().rev() {
        for (glob, props) in sections {
            let Some(glob) = glob else { continue };
            if glob.is_empty() {
                continue;
            }
            let prefix = cur.to_str()?.trim_end_matches('/');
            let full = match glob.find('/') {
                None => format!("{prefix}/**/{glob}"),
                Some(0) => format!("{prefix}{glob}"),
                Some(_) => format!("{prefix}/{glob}"),
            };
            if !glob_matches(&full, file_str, true)? {
                continue;
            }
            for (k, v) in props {
                let key = k.to_lowercase();
                matches.insert(key.clone(), editorconfig_value(&key, v));
            }
        }
    }
    let is_str =
        |m: &Map<String, Value>, k: &str, s: &str| m.get(k).and_then(Value::as_str) == Some(s);
    if is_str(&matches, "indent_style", "tab") && !matches.contains_key("indent_size") {
        matches.insert("indent_size".into(), Value::String("tab".into()));
    }
    if matches.contains_key("indent_size")
        && !matches.contains_key("tab_width")
        && !is_str(&matches, "indent_size", "tab")
    {
        let v = matches["indent_size"].clone();
        matches.insert("tab_width".into(), v);
    }
    if matches.contains_key("indent_size")
        && matches.contains_key("tab_width")
        && is_str(&matches, "indent_size", "tab")
    {
        let v = matches["tab_width"].clone();
        matches.insert("indent_size".into(), v);
    }
    matches.retain(|_, v| v.as_str() != Some("unset"));
    let mut result = Map::new();
    if matches.is_empty() {
        return Some(result);
    }
    let get = |k: &str| matches.get(k);
    let truthy = |v: Option<&Value>| match v {
        None | Some(Value::Null) | Some(Value::Bool(false)) => false,
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0),
        _ => true,
    };
    if truthy(get("indent_style")) {
        result.insert(
            "useTabs".into(),
            Value::Bool(get("indent_style").and_then(Value::as_str) == Some("tab")),
        );
    }
    if get("indent_size").and_then(Value::as_str) == Some("tab") {
        result.insert("useTabs".into(), Value::Bool(true));
    }
    let use_tabs = result.get("useTabs") == Some(&Value::Bool(true));
    if use_tabs && truthy(get("tab_width")) {
        result.insert("tabWidth".into(), get("tab_width")?.clone());
    } else if get("indent_style").and_then(Value::as_str) == Some("space")
        && truthy(get("indent_size"))
        && get("indent_size").and_then(Value::as_str) != Some("tab")
    {
        result.insert("tabWidth".into(), get("indent_size")?.clone());
    } else if let Some(v) = get("tab_width") {
        result.insert("tabWidth".into(), v.clone());
    }
    if truthy(get("max_line_length")) {
        if get("max_line_length").and_then(Value::as_str) == Some("off") {
            return None;
        }
        result.insert("printWidth".into(), get("max_line_length")?.clone());
    }
    match get("quote_type").and_then(Value::as_str) {
        Some("single") => {
            result.insert("singleQuote".into(), Value::Bool(true));
        }
        Some("double") => {
            result.insert("singleQuote".into(), Value::Bool(false));
        }
        _ => {}
    }
    if let Some(eol) = get("end_of_line").and_then(Value::as_str)
        && matches!(eol, "cr" | "crlf" | "lf")
    {
        result.insert("endOfLine".into(), Value::String(eol.into()));
    }
    Some(result)
}

fn prettier_config(dir: &Path, file: &Path) -> Option<Option<Map<String, Value>>> {
    let mut d = Some(dir);
    while let Some(cur) = d {
        for name in CONFIG_FILES {
            let path = cur.join(name);
            if !std::fs::metadata(&path).is_ok_and(|m| m.is_file()) {
                continue;
            }
            let text = String::from_utf8(std::fs::read(&path).ok()?).ok()?;
            let config = match name {
                "package.json" => {
                    let Ok(v) = serde_json::from_str::<Value>(&text) else {
                        continue;
                    };
                    match v.get("prettier") {
                        None | Some(Value::Null) | Some(Value::Bool(false)) => continue,
                        Some(Value::String(s)) if s.is_empty() => continue,
                        Some(Value::Number(n)) if n.as_f64() == Some(0.0) => continue,
                        Some(p) => p.clone(),
                    }
                }
                ".prettierrc" => {
                    if text.trim().is_empty() {
                        return Some(Some(Map::new()));
                    }
                    serde_json::from_str::<Value>(&text).ok()?
                }
                ".prettierrc.json" => serde_json::from_str::<Value>(&text).ok()?,
                _ => return None,
            };
            let Value::Object(mut config) = config else {
                return None;
            };
            config.remove("$schema");
            let overrides = config.remove("overrides");
            if let Some(overrides) = overrides {
                let rel = relative(cur, file)?;
                let Value::Array(list) = overrides else {
                    return None;
                };
                for o in list {
                    if path_matches(&rel, o.get("files")?, o.get("excludeFiles"))? {
                        let Some(Value::Object(opts)) = o.get("options") else {
                            return None;
                        };
                        for (k, v) in opts {
                            config.insert(k.clone(), v.clone());
                        }
                    }
                }
            }
            return Some(Some(config));
        }
        d = cur.parent();
    }
    Some(None)
}

fn pattern_list(v: &Value) -> Option<Vec<String>> {
    match v {
        Value::String(s) => Some(vec![s.clone()]),
        Value::Array(items) => items.iter().map(|x| x.as_str().map(String::from)).collect(),
        _ => None,
    }
}

fn matches_any(target: &str, patterns: &[String]) -> Option<bool> {
    for p in patterns {
        if p.starts_with("./") || p.starts_with('/') {
            return None;
        }
        if glob_matches(p, target, false)? {
            return Some(true);
        }
    }
    Some(false)
}

fn path_matches(rel: &str, files: &Value, exclude: Option<&Value>) -> Option<bool> {
    let patterns = pattern_list(files)?;
    let excluded = match exclude {
        Some(e) => pattern_list(e)?,
        None => Vec::new(),
    };
    let base = rel.rsplit('/').next().unwrap_or(rel);
    let (with_slashes, without): (Vec<String>, Vec<String>) =
        patterns.into_iter().partition(|p| p.contains('/'));
    let first = matches_any(base, &without)? && !matches_any(base, &excluded)?;
    let second = matches_any(rel, &with_slashes)? && !matches_any(rel, &excluded)?;
    Some(first || second)
}

fn to_args(options: &Map<String, Value>) -> Option<Vec<String>> {
    let mut args = Vec::new();
    let flag = |args: &mut Vec<String>, on: bool, name: &str| {
        if on {
            args.push(name.to_string());
        }
    };
    let int = |v: &Value| v.as_u64().filter(|n| *n <= i32::MAX as u64);
    let one_of =
        |v: &Value, allowed: &[&str]| v.as_str().filter(|s| allowed.contains(s)).map(String::from);
    for (key, value) in options {
        match key.as_str() {
            "printWidth" => args.extend(["--print-width".into(), int(value)?.to_string()]),
            "tabWidth" => args.extend(["--tab-width".into(), int(value)?.to_string()]),
            "useTabs" => flag(&mut args, value.as_bool()?, "--use-tabs"),
            "semi" => flag(&mut args, !value.as_bool()?, "--no-semi"),
            "singleQuote" => flag(&mut args, value.as_bool()?, "--single-quote"),
            "jsxSingleQuote" => flag(&mut args, value.as_bool()?, "--jsx-single-quote"),
            "bracketSpacing" => flag(&mut args, !value.as_bool()?, "--no-bracket-spacing"),
            "bracketSameLine" => flag(&mut args, value.as_bool()?, "--bracket-same-line"),
            "experimentalTernaries" => {
                flag(&mut args, value.as_bool()?, "--experimental-ternaries")
            }
            "trailingComma" => args.extend([
                "--trailing-comma".into(),
                one_of(value, &["all", "es5", "none"])?,
            ]),
            "arrowParens" => args.extend([
                "--arrow-parens".into(),
                one_of(value, &["always", "avoid"])?,
            ]),
            "quoteProps" => args.extend([
                "--quote-props".into(),
                one_of(value, &["as-needed", "consistent", "preserve"])?,
            ]),
            "objectWrap" => args.extend([
                "--object-wrap".into(),
                one_of(value, &["preserve", "collapse"])?,
            ]),
            "experimentalOperatorPosition" => args.extend([
                "--experimental-operator-position".into(),
                one_of(value, &["start", "end"])?,
            ]),
            "endOfLine" => {
                one_of(value, &["lf"])?;
            }
            "proseWrap" => {
                one_of(value, &["always", "never", "preserve"])?;
            }
            "htmlWhitespaceSensitivity" => {
                one_of(value, &["css", "strict", "ignore"])?;
            }
            "vueIndentScriptAndStyle" => {
                value.as_bool()?;
            }
            _ => return None,
        }
    }
    Some(args)
}
