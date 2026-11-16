use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "carcinize",
    version,
    about = "Plan and check incremental TypeScript/JavaScript to Rust migrations"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Map a repository and write a ranked, ordered migration plan.
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
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Analyze { root, json, out } => {
            let report = carcinize::analyze::analyze(&root)
                .with_context(|| format!("analyzing {}", root.display()))?;
            let text = if json {
                serde_json::to_string_pretty(&report)? + "\n"
            } else {
                let title = root
                    .canonicalize()
                    .ok()
                    .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                    .unwrap_or_else(|| root.display().to_string());
                carcinize::report::markdown(&report, &title)
            };
            match out {
                Some(path) => std::fs::write(&path, text)
                    .with_context(|| format!("writing {}", path.display()))?,
                None => print!("{text}"),
            }
        }
    }
    Ok(())
}
