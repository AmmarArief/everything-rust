fn main() {
    let path = std::env::args().nth(1).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    let alloc = oxc_allocator::Allocator::default();
    let st = oxc_span::SourceType::from_path(&path).unwrap();
    let ret = oxc_parser::Parser::new(&alloc, &text, st)
        .with_options(oxc_parser::ParseOptions {
            preserve_parens: true,
            ..Default::default()
        })
        .parse();
    println!(
        "{}",
        ret.program.to_pretty_estree_json(st.is_typescript(), true)
    );
}
