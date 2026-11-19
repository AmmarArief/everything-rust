fn main() {
    let path = std::env::args().nth(1).unwrap_or_default();
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let name = std::path::Path::new(&path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let Some(opts) = prettier_js_rs::Options::from_cli(&["--stdin-filepath".to_string(), name])
    else {
        return;
    };
    match prettier_js_rs::format(&text, &opts) {
        Ok(out) => print!("{out}"),
        Err(e) => println!("ERR {}", e.0),
    }
}
