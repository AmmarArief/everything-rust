use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let inputs = std::fs::read_to_string(&args[1]).expect("inputs.jsonl");
    let only: Option<std::collections::HashSet<String>> = args
        .get(2)
        .map(|f| std::fs::read_to_string(f).expect("names").lines().map(String::from).collect());
    let reps: u32 = args.get(3).and_then(|r| r.parse().ok()).unwrap_or(20);
    let mut total = 0f64;
    for line in inputs.lines().filter(|l| !l.is_empty()) {
        let case: serde_json::Value = serde_json::from_str(line).expect("case");
        let name = case["name"].as_str().unwrap_or_default();
        if only.as_ref().is_some_and(|o| !o.contains(name)) {
            continue;
        }
        let argv: Vec<String> = case["args"]
            .as_array()
            .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
            .unwrap_or_default();
        let stdin = case["stdin"].as_str().unwrap_or_default();
        let Some(opts) = prettier_js_rs::Options::from_cli(&argv) else {
            continue;
        };
        if prettier_js_rs::format(stdin, &opts).is_err() {
            continue;
        }
        let start = Instant::now();
        for _ in 0..reps {
            let _ = std::hint::black_box(prettier_js_rs::format(stdin, &opts));
        }
        let us = start.elapsed().as_secs_f64() * 1e6 / f64::from(reps);
        total += us;
        println!("{name}\t{}\t{us:.1}", stdin.len());
    }
    eprintln!("total_us\t{total:.0}");
}
