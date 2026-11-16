use std::io::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;

/// `println!` that stops quietly when stdout is closed (`carcinize ... | head`).
macro_rules! say {
    ($($arg:tt)*) => {{
        let _ = writeln!(std::io::stdout().lock(), $($arg)*);
    }};
}

macro_rules! say_raw {
    ($($arg:tt)*) => {{
        let _ = write!(std::io::stdout().lock(), $($arg)*);
    }};
}

use anyhow::{Context, Result};
use carcinize::{analyze, bridge, gate, harness, mcp, report, shadow};
use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(
    name = "carcinize",
    version,
    about = "Plan, bridge, verify and ship incremental TypeScript/JavaScript to Rust migrations",
    after_help = "Loop per unit: analyze -> bridge -> record -> port src/port.rs -> build -> diff -> shadow -> gate"
)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Clone, Copy, ValueEnum)]
enum KindArg {
    Napi,
    Wasm,
}

#[derive(Subcommand)]
enum Cmd {
    /// 1. Map a repository and write a ranked, ordered migration plan.
    Analyze {
        /// Repository root.
        #[arg(default_value = ".")]
        root: PathBuf,
        /// Print the machine-readable report (schema `carcinize/analyze@1`) instead of Markdown.
        #[arg(long)]
        json: bool,
        /// Write to this file instead of stdout.
        #[arg(long, short)]
        out: Option<PathBuf>,
        /// Source language; detected from the repository when omitted.
        #[arg(long, value_enum)]
        lang: Option<carcinize::lang::Lang>,
    },
    /// 2. Scaffold a Rust crate with JS bindings for one unit.
    Bridge {
        /// Repository root.
        root: PathBuf,
        /// Unit id, as `analyze` lists it.
        unit: String,
        /// Bridge to generate; defaults to the plan's choice.
        #[arg(long, value_enum)]
        kind: Option<KindArg>,
        /// Crate folder; defaults to `<root>/rust/<name>`.
        #[arg(long, short)]
        out: Option<PathBuf>,
        /// Crate name; defaults to `<unit>-rs`.
        #[arg(long)]
        name: Option<String>,
        /// Bridge a unit the plan marks blocked or keep.
        #[arg(long)]
        force: bool,
        /// Keep an export in TS: `name` or `"name: reason"`. Repeatable.
        #[arg(long)]
        keep: Vec<String>,
    },
    /// 2b. Regenerate the bindings (`lib.rs`, `index.cjs`, `js.rs`) from the signatures in `port.rs`.
    Refresh {
        #[arg(default_value = ".")]
        crate_dir: PathBuf,
    },
    /// 3a. Record what the TS functions return on generated and given inputs.
    Record {
        /// Crate folder made by `bridge`.
        #[arg(default_value = ".")]
        crate_dir: PathBuf,
        /// Generated cases per function.
        #[arg(long, default_value_t = 200)]
        count: usize,
        /// Extra cases: JSONL of `{"fn": name, "args": [...]}`.
        #[arg(long)]
        inputs: Option<PathBuf>,
        /// Import the original from this JS module (e.g. built output) instead of the TS source.
        #[arg(long)]
        module: Option<PathBuf>,
        /// Resolve npm packages from this folder's `node_modules` (when the repo has none installed).
        #[arg(long)]
        resolve_from: Option<PathBuf>,
        /// Extra arguments for node, e.g. `--node-arg=--import=tsx`.
        #[arg(long = "node-arg", allow_hyphen_values = true)]
        node_args: Vec<String>,
    },
    /// 3b. Build the crate's JS module (napi-rs `.node` or wasm-bindgen output).
    Build {
        #[arg(default_value = ".")]
        crate_dir: PathBuf,
    },
    /// 3c. Replay recorded cases against the Rust module.
    Diff {
        #[arg(default_value = ".")]
        crate_dir: PathBuf,
        /// Test this JS module instead of the crate's `index.cjs`.
        #[arg(long)]
        module: Option<PathBuf>,
        /// Compare objects regardless of key order.
        #[arg(long)]
        ignore_key_order: bool,
        #[arg(long)]
        json: bool,
        #[arg(long = "node-arg", allow_hyphen_values = true)]
        node_args: Vec<String>,
    },
    /// 3d. Time the recorded cases on the TS original and on Rust, bridge included.
    Bench {
        #[arg(default_value = ".")]
        crate_dir: PathBuf,
        /// Minimum measured time per function and side, in milliseconds.
        #[arg(long, default_value_t = 200)]
        min_ms: u64,
        /// Resolve npm packages the original imports from this folder's `node_modules`.
        #[arg(long)]
        resolve_from: Option<PathBuf>,
        #[arg(long)]
        json: bool,
        #[arg(long = "node-arg", allow_hyphen_values = true)]
        node_args: Vec<String>,
    },
    /// 4. Generate a drop-in module that runs TS and Rust side by side.
    Shadow {
        #[arg(default_value = ".")]
        crate_dir: PathBuf,
        /// Where to write the module; defaults to `<crate>/shadow/<name>.shadow.ts`.
        #[arg(long, short)]
        out: Option<PathBuf>,
    },
    /// 5. Check the definition of done for a port.
    Gate {
        #[arg(default_value = ".")]
        crate_dir: PathBuf,
        #[arg(long)]
        json: bool,
        #[arg(long = "node-arg", allow_hyphen_values = true)]
        node_args: Vec<String>,
    },
    /// Serve every step as MCP tools over stdio.
    Mcp,
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<ExitCode> {
    match Cli::parse().command {
        Cmd::Analyze {
            root,
            json,
            out,
            lang,
        } => {
            let lang = lang.unwrap_or_else(|| carcinize::lang::Lang::detect(&root));
            let report = analyze::analyze_lang(&root, lang)
                .with_context(|| format!("analyzing {}", root.display()))?;
            let text = if json {
                serde_json::to_string_pretty(&report)? + "\n"
            } else {
                let title = root
                    .canonicalize()
                    .ok()
                    .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                    .unwrap_or_else(|| root.display().to_string());
                report::markdown(&report, &title)
            };
            match out {
                Some(path) => std::fs::write(&path, text)
                    .with_context(|| format!("writing {}", path.display()))?,
                None => say_raw!("{text}"),
            }
        }
        Cmd::Bridge {
            root,
            unit,
            kind,
            out,
            name,
            force,
            keep,
        } => {
            let outcome = bridge::bridge(&bridge::Options {
                root: &root,
                unit: &unit,
                kind: kind.map(|k| match k {
                    KindArg::Napi => bridge::Kind::Napi,
                    KindArg::Wasm => bridge::Kind::Wasm,
                }),
                out,
                name,
                force,
                keep,
                lang: None,
            })?;
            say!(
                "Created {} ({:?}) for {}",
                outcome.crate_dir.display(),
                outcome.kind,
                outcome.exports.join(", ")
            );
            for file in &outcome.files {
                say!("  {file}");
            }
            for note in &outcome.notes {
                say!("- {note}");
            }
        }
        Cmd::Record {
            crate_dir,
            count,
            inputs,
            module,
            resolve_from,
            node_args,
        } => {
            let outcome = harness::record(&harness::RecordOptions {
                crate_dir: &crate_dir,
                count,
                inputs: inputs.as_deref(),
                module: module.as_deref(),
                resolve_from: resolve_from.as_deref(),
                node_args: &node_args,
            })?;
            say!(
                "Recorded {} cases ({} threw) in {}",
                outcome.cases,
                outcome.errors,
                outcome.path.display()
            );
            for (name, n) in &outcome.per_function {
                say!("  {name}: {n}");
            }
        }
        Cmd::Refresh { crate_dir } => {
            let r = bridge::refresh(&crate_dir)?;
            say!(
                "Regenerated {} for {}",
                r.files.join(", "),
                r.functions.join(", ")
            );
        }
        Cmd::Build { crate_dir } => {
            say!("Built {}", harness::build(&crate_dir)?.display());
        }
        Cmd::Diff {
            crate_dir,
            module,
            ignore_key_order,
            json,
            node_args,
        } => {
            let d = harness::diff(&harness::DiffOptions {
                crate_dir: &crate_dir,
                module: module.as_deref(),
                ignore_key_order,
                node_args: &node_args,
            })?;
            if json {
                say!("{}", serde_json::to_string_pretty(&d)?);
            } else {
                say!("{}/{} cases match", d.passed, d.total);
                if !d.missing.is_empty() {
                    say!("Missing exports: {}", d.missing.join(", "));
                }
                for m in &d.mismatches {
                    say!(
                        "- {}({}): expected {}, got {}",
                        m["fn"].as_str().unwrap_or_default(),
                        m["args"]
                            .as_array()
                            .map(|a| a
                                .iter()
                                .map(ToString::to_string)
                                .collect::<Vec<_>>()
                                .join(", "))
                            .unwrap_or_default(),
                        m["expected"],
                        m["actual"]
                    );
                }
            }
            if d.failed > 0 {
                return Ok(ExitCode::FAILURE);
            }
        }
        Cmd::Bench {
            crate_dir,
            min_ms,
            resolve_from,
            json,
            node_args,
        } => {
            let rows = harness::bench(&harness::BenchOptions {
                crate_dir: &crate_dir,
                min_ms,
                resolve_from: resolve_from.as_deref(),
                node_args: &node_args,
            })?;
            if json {
                say!("{}", serde_json::to_string_pretty(&rows)?);
            } else {
                say!(
                    "{:<24} {:>6} {:>12} {:>12} {:>9}",
                    "function",
                    "cases",
                    "TS ns/call",
                    "Rust ns/call",
                    "speedup"
                );
                for r in &rows {
                    say!(
                        "{:<24} {:>6} {:>12.0} {:>12.0} {:>8.2}x",
                        r.name,
                        r.cases,
                        r.original_ns,
                        r.rust_ns,
                        r.speedup
                    );
                }
                say!(
                    "Bridge included. Below 1x, Rust is slower than the original as called from JS."
                );
            }
        }
        Cmd::Shadow { crate_dir, out } => {
            let outcome = shadow::shadow(&crate_dir, out.as_deref())?;
            say!(
                "Wrote {} ({})",
                outcome.path.display(),
                outcome.exports.join(", ")
            );
            if outcome.path.extension().is_some_and(|e| e == "c") {
                say!(
                    "Compile the original with the -D flags in shadow/flags.txt, build the crate with --features shadow, and link all three."
                );
            } else {
                say!(
                    "Import it in place of the original. CARCINIZE_MODE=old|shadow|new switches at runtime."
                );
            }
        }
        Cmd::Gate {
            crate_dir,
            json,
            node_args,
        } => {
            let outcome = gate::gate(&crate_dir, &node_args)?;
            if json {
                say!("{}", serde_json::to_string_pretty(&outcome)?);
            } else {
                for check in &outcome.checks {
                    let mark = if check.passed { "pass" } else { "FAIL" };
                    let first = check.detail.lines().next().unwrap_or_default();
                    say!("{mark}  {:<10} {first}", check.name);
                    if !check.passed {
                        for line in check.detail.lines().skip(1) {
                            say!("            {line}");
                        }
                    }
                }
                say!(
                    "{}",
                    if outcome.passed {
                        "Gate passed."
                    } else {
                        "Gate failed."
                    }
                );
            }
            if !outcome.passed {
                return Ok(ExitCode::FAILURE);
            }
        }
        Cmd::Mcp => mcp::serve()?,
    }
    Ok(ExitCode::SUCCESS)
}
