//! `compilerOptions.paths` from the nearest `tsconfig.json`: import aliases like `~/utils`.

use std::path::Path;

use serde_json::Value;

/// Alias patterns, with targets as root-relative paths (`*` kept where the pattern had one).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PathAliases {
    pub entries: Vec<(String, Vec<String>)>,
}

impl PathAliases {
    /// The tsconfig closest to `dir` (root-relative), walking up to the root.
    pub fn nearest(root: &Path, dir: &str) -> Self {
        let mut current = dir.trim_end_matches('/').to_string();
        loop {
            let candidate = if current.is_empty() {
                "tsconfig.json".to_string()
            } else {
                format!("{current}/tsconfig.json")
            };
            if root.join(&candidate).is_file() {
                return load(root, &candidate, 0)
                    .map(|c| c.aliases)
                    .unwrap_or_default();
            }
            if current.is_empty() {
                return Self::default();
            }
            current = current
                .rsplit_once('/')
                .map_or(String::new(), |(parent, _)| parent.to_string());
        }
    }

    /// Root-relative base paths an import specifier may point at, in tsconfig order.
    pub fn candidates(&self, specifier: &str) -> Vec<String> {
        let mut best: Option<(usize, &Vec<String>, String)> = None;
        for (pattern, targets) in &self.entries {
            let matched = match pattern.split_once('*') {
                Some((prefix, suffix)) => specifier
                    .strip_prefix(prefix)
                    .and_then(|rest| rest.strip_suffix(suffix))
                    .map(|star| (prefix.len(), star.to_string())),
                None => (pattern == specifier).then(|| (usize::MAX, String::new())),
            };
            // TS picks the pattern with the longest prefix.
            if let Some((weight, star)) = matched
                && best.as_ref().is_none_or(|(w, _, _)| weight > *w)
            {
                best = Some((weight, targets, star));
            }
        }
        best.map(|(_, targets, star)| targets.iter().map(|t| t.replacen('*', &star, 1)).collect())
            .unwrap_or_default()
    }
}

struct Config {
    aliases: PathAliases,
}

/// Reads a tsconfig (JSON with comments), following relative `extends`.
fn load(root: &Path, rel: &str, depth: usize) -> Option<Config> {
    if depth > 8 {
        return None;
    }
    let text = std::fs::read_to_string(root.join(rel)).ok()?;
    let json: Value = json5::from_str(&text).ok()?;
    let dir = rel.rsplit_once('/').map_or("", |(d, _)| d);
    let mut inherited = PathAliases::default();
    let extends: Vec<&str> = match &json["extends"] {
        Value::String(s) => vec![s.as_str()],
        Value::Array(items) => items.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    };
    for parent in extends.into_iter().filter(|p| p.starts_with('.')) {
        let mut target = join(dir, parent);
        if !target.ends_with(".json") {
            target.push_str(".json");
        }
        if let Some(parent) = load(root, &target, depth + 1) {
            inherited = parent.aliases;
        }
    }
    let options = &json["compilerOptions"];
    let Some(paths) = options["paths"].as_object() else {
        return Some(Config { aliases: inherited });
    };
    // Targets are relative to `baseUrl` when set, else to the config's own folder.
    let base = options["baseUrl"]
        .as_str()
        .map_or_else(|| dir.to_string(), |b| join(dir, b));
    let entries = paths
        .iter()
        .map(|(pattern, targets)| {
            let targets = targets
                .as_array()
                .map(|t| {
                    t.iter()
                        .filter_map(Value::as_str)
                        .map(|t| join(&base, t))
                        .collect()
                })
                .unwrap_or_default();
            (pattern.clone(), targets)
        })
        .collect();
    Some(Config {
        aliases: PathAliases { entries },
    })
}

/// `dir/rel`, with `.` and `..` collapsed; root-relative.
fn join(dir: &str, rel: &str) -> String {
    let mut parts: Vec<&str> = dir.split('/').filter(|p| !p.is_empty()).collect();
    for part in rel.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(part),
        }
    }
    parts.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_like_typescript() {
        let aliases = PathAliases {
            entries: vec![
                ("~/*".into(), vec!["src/*".into()]),
                (
                    "@lib/*".into(),
                    vec!["packages/lib/src/*".into(), "vendor/lib/*".into()],
                ),
                ("@lib/special".into(), vec!["src/special.ts".into()]),
            ],
        };
        assert_eq!(aliases.candidates("~/util/strings"), ["src/util/strings"]);
        assert_eq!(
            aliases.candidates("@lib/a"),
            ["packages/lib/src/a", "vendor/lib/a"]
        );
        assert_eq!(aliases.candidates("@lib/special"), ["src/special.ts"]);
        assert!(aliases.candidates("lodash").is_empty());
    }
}
