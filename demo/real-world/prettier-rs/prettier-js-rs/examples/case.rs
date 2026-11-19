fn main() {
    let stem = std::env::args().nth(1).unwrap_or_default();
    let toml = std::fs::read_to_string(format!("{stem}.toml")).unwrap_or_default();
    let args_line = toml
        .lines()
        .find(|l| l.starts_with("args = "))
        .unwrap_or("args = []");
    let args: Vec<String> = serde_json::from_str(&args_line["args = ".len()..]).unwrap_or_default();
    let stdin = std::fs::read_to_string(format!("{stem}.stdin")).unwrap_or_default();
    let Some(opts) = prettier_js_rs::Options::from_cli(&args) else {
        println!("NO OPTIONS");
        return;
    };
    match prettier_js_rs::format(&stdin, &opts) {
        Ok(out) => print!("{out}"),
        Err(e) => println!("ERR {}", e.0),
    }
}
