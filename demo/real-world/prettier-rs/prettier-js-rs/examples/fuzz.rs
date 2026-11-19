fn main() {
    let path = std::env::args().nth(1).expect("jsonl");
    let data = std::fs::read_to_string(path).expect("read");
    let (mut rust_ok, mut bad, mut total) = (0, 0, 0);
    for line in data.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        total += 1;
        let fp = v["filepath"].as_str().unwrap_or_default();
        let text = v["text"].as_str().unwrap_or_default();
        let Some(opts) = prettier_js_rs::Options::for_path(fp) else {
            continue;
        };
        if let Ok(out) = prettier_js_rs::format(text, &opts) {
            rust_ok += 1;
            if !v["ok"].as_bool().unwrap_or(false) || v["out"].as_str() != Some(out.as_str()) {
                bad += 1;
                if let Ok(dir) = std::env::var("FUZZDIR") {
                    let _ = std::fs::create_dir_all(&dir);
                    let ext = fp.rsplit('.').next().unwrap_or("txt");
                    let _ = std::fs::write(format!("{dir}/{bad}.{ext}"), text);
                }
                if bad
                    <= std::env::var("NSHOW")
                        .ok()
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(5)
                {
                    eprintln!(
                        "=== MISMATCH prettier_ok={} {}\n{}\n--- rust\n{}\n--- prettier\n{}",
                        v["ok"],
                        fp,
                        text,
                        out,
                        v["out"].as_str().unwrap_or("(error)")
                    );
                }
            }
        }
    }
    println!("total {total} rust_ok {rust_ok} mismatch {bad}");
}
