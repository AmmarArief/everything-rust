//! Glue from the front door to `js-format-rs`: `prettier --stdin-filepath <file> [flags]` for a
//! JavaScript or TypeScript file the Rust printer covers; anything else goes to Prettier.

use std::ffi::OsString;
use std::io::{Read, Write};
use std::process::ExitCode;

use clap::ArgMatches;

mod config;
use js_format_rs::port::printer;

pub fn run(_command: &str, _matches: &ArgMatches) -> ExitCode {
    let argv: Vec<OsString> = std::env::args_os().collect();
    let args: Option<Vec<String>> = argv[1..]
        .iter()
        .map(|a| a.to_str().map(String::from))
        .collect();
    let Some(opts) = args.as_deref().and_then(printer::Options::from_cli) else {
        return crate::original::run(&argv[1..]);
    };
    let Some(mut merged) = config::resolve(&opts.filepath) else {
        return crate::original::run(&argv[1..]);
    };
    merged.extend(args.iter().flatten().cloned());
    let Some(opts) = printer::Options::from_cli(&merged) else {
        return crate::original::run(&argv[1..]);
    };
    let mut input = Vec::new();
    if std::io::stdin().read_to_end(&mut input).is_err() {
        return crate::original::run_with_stdin(&argv[1..], &input);
    }
    let Ok(text) = std::str::from_utf8(&input) else {
        return crate::original::run_with_stdin(&argv[1..], &input);
    };
    match printer::format(text, &opts) {
        Ok(formatted) => {
            let mut out = std::io::stdout().lock();
            if out
                .write_all(formatted.as_bytes())
                .and_then(|()| out.flush())
                .is_err()
            {
                return ExitCode::from(2);
            }
            ExitCode::SUCCESS
        }
        Err(_) => crate::original::run_with_stdin(&argv[1..], &input),
    }
}
