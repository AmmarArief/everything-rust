use std::collections::BTreeMap;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cases_dir = std::path::Path::new(&args[1]);
    let show = args.get(2).map(String::as_str);
    let mut reasons: BTreeMap<String, usize> = BTreeMap::new();
    let (mut ok, mut bad, mut skipped, mut total) = (0, 0, 0, 0);
    let mut bad_names = Vec::new();
    let mut entries: Vec<_> = std::fs::read_dir(cases_dir)
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
        if toml.contains("fs.base")
            || path
                .file_name()
                .is_some_and(|f| f.to_string_lossy().starts_with("config-"))
        {
            continue;
        }
        let Some(args_line) = toml.lines().find(|l| l.starts_with("args")) else {
            continue;
        };
        let Ok(argv) = parse_args(args_line) else {
            continue;
        };
        total += 1;
        let stem = path.with_extension("");
        let stdin = std::fs::read_to_string(stem.with_extension("stdin")).unwrap_or_default();
        let expected = std::fs::read_to_string(stem.with_extension("stdout")).unwrap_or_default();
        let status_ok = !toml.contains("status") || toml.contains("status.code = 0");
        if std::env::var("TSONLY").is_ok()
            && !argv.iter().any(|a| {
                let a = a.to_ascii_lowercase();
                a.ends_with(".ts")
                    || a.ends_with(".tsx")
                    || a.ends_with(".mts")
                    || a.ends_with(".cts")
            })
        {
            continue;
        }
        let Some(opts) = prettier_js_rs::Options::from_cli(&argv) else {
            skipped += 1;
            *reasons.entry("cli flags".into()).or_default() += 1;
            continue;
        };
        let started = std::time::Instant::now();
        let result = std::panic::catch_unwind(|| prettier_js_rs::format(&stdin, &opts));
        let elapsed = started.elapsed();
        if elapsed.as_millis() > 500 {
            eprintln!("slow {} {:?}", stem.display(), elapsed);
        }
        match result {
            Ok(Ok(out)) => {
                if status_ok && out == expected {
                    ok += 1;
                    if let Ok(list) = std::env::var("CLAIMED") {
                        use std::io::Write;
                        let mut f = std::fs::OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(list)
                            .unwrap();
                        let _ = writeln!(f, "{}", stem.file_name().unwrap().to_string_lossy());
                    }
                } else {
                    bad += 1;
                    let name = stem.file_name().unwrap().to_string_lossy().to_string();
                    if let Some(dir) = show {
                        let _ = std::fs::create_dir_all(dir);
                        let _ = std::fs::write(format!("{dir}/{name}.expected"), &expected);
                        let _ = std::fs::write(format!("{dir}/{name}.got"), &out);
                    }
                    bad_names.push(name);
                }
            }
            Ok(Err(e)) => {
                skipped += 1;
                let key = e.0.split(':').next().unwrap_or("").to_string();
                if std::env::var("SHOWBAIL").is_ok_and(|v| key.contains(&v)) {
                    eprintln!("BAIL {} {}", stem.display(), e.0);
                }
                *reasons.entry(key).or_default() += 1;
            }
            Err(_) => {
                bad += 1;
                bad_names.push(format!("PANIC {}", stem.display()));
            }
        }
    }
    let mut reasons: Vec<_> = reasons.into_iter().collect();
    reasons.sort_by_key(|r| std::cmp::Reverse(r.1));
    for (r, n) in reasons.iter().take(40) {
        println!("{n:6} {r}");
    }
    for n in &bad_names {
        println!("MISMATCH {n}");
    }
    println!("total {total} rust-ok {ok} mismatch {bad} fallback {skipped}");
}

fn parse_args(line: &str) -> Result<Vec<String>, ()> {
    let start = line.find('[').ok_or(())?;
    let v: serde_json::Value = serde_json::from_str(&line[start..]).map_err(|_| ())?;
    Ok(v.as_array()
        .ok_or(())?
        .iter()
        .filter_map(|x| x.as_str().map(String::from))
        .collect())
}
