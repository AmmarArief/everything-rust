use std::path::PathBuf;
use std::process::ExitCode;

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
        Cmd::Analyze { root, json, out } => {
            let report =
                analyze::analyze(&root).with_context(|| format!("analyzing {}", root.display()))?;
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
                None => print!("{text}"),
            }
        }
        Cmd::Bridge {
            root,
            unit,
            kind,
            out,
            name,
            force,
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
            })?;
            println!(
                "Created {} ({:?}) for {}",
                outcome.crate_dir.display(),
                outcome.kind,
                outcome.exports.join(", ")
            );
            for file in &outcome.files {
                println!("  {file}");
            }
            for note in &outcome.notes {
                println!("- {note}");
            }
        }
        Cmd::Record {
            crate_dir,
            count,
            inputs,
            module,
            node_args,
        } => {
            let outcome = harness::record(&harness::RecordOptions {
                crate_dir: &crate_dir,
                count,
                inputs: inputs.as_deref(),
                module: module.as_deref(),
                node_args: &node_args,
            })?;
            println!(
                "Recorded {} cases ({} threw) in {}",
                outcome.cases,
                outcome.errors,
                outcome.path.display()
            );
            for (name, n) in &outcome.per_function {
                println!("  {name}: {n}");
            }
        }
        Cmd::Build { crate_dir } => {
            println!("Built {}", harness::build(&crate_dir)?.display());
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
                println!("{}", serde_json::to_string_pretty(&d)?);
            } else {
                println!("{}/{} cases match", d.passed, d.total);
                if !d.missing.is_empty() {
                    println!("Missing exports: {}", d.missing.join(", "));
                }
                for m in &d.mismatches {
                    println!(
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
        Cmd::Shadow { crate_dir, out } => {
            let outcome = shadow::shadow(&crate_dir, out.as_deref())?;
            println!(
                "Wrote {} ({})",
                outcome.path.display(),
                outcome.exports.join(", ")
            );
            println!(
                "Import it in place of the original. CARCINIZE_MODE=old|shadow|new switches at runtime."
            );
        }
        Cmd::Gate {
            crate_dir,
            json,
            node_args,
        } => {
            let outcome = gate::gate(&crate_dir, &node_args)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&outcome)?);
            } else {
                for check in &outcome.checks {
                    let mark = if check.passed { "pass" } else { "FAIL" };
                    let first = check.detail.lines().next().unwrap_or_default();
                    println!("{mark}  {:<10} {first}", check.name);
                    if !check.passed {
                        for line in check.detail.lines().skip(1) {
                            println!("            {line}");
                        }
                    }
                }
                println!(
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
