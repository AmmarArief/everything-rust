//! The front door: `rust/app`, a Rust binary that owns a CLI's entry point from day one and runs
//! the original for every command not ported yet, and `wire`, which hands commands to a unit's
//! `port.rs` behind a kill switch.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use toml_edit::{DocumentMut, InlineTable, value};

use crate::app::{self, ReplayOptions, ReplayOutcome};

pub const APP_DIR: &str = "rust/app";
pub const STATE_FILE: &str = "rust-anything.json";
pub const KILL_SWITCH_PREFIX: &str = "RUST_ANYTHING_UNIT_";
const SCHEMA: &str = "rust-anything/front-door@1";
pub(crate) const GLUE_MARKER: &str = "unimplemented!(\"glue for";

/// `rust/app/rust-anything.json`: the commands the front door routes and the units wired to them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct State {
    pub schema: String,
    pub name: String,
    pub commands: Vec<String>,
    #[serde(default)]
    pub units: Vec<WiredUnit>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub lifted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WiredUnit {
    pub name: String,
    #[serde(rename = "crate")]
    pub crate_name: String,
    pub path: String,
    pub commands: Vec<String>,
    pub switch: String,
}

impl State {
    pub fn load(repo: &Path) -> Result<State> {
        let path = repo.join(APP_DIR).join(STATE_FILE);
        let text = std::fs::read_to_string(&path).with_context(|| {
            format!(
                "reading {}: run `rust-anything app front-door` first",
                path.display()
            )
        })?;
        serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
    }

    fn save(&self, repo: &Path) -> Result<()> {
        let path = repo.join(APP_DIR).join(STATE_FILE);
        std::fs::write(&path, serde_json::to_string_pretty(self)? + "\n")
            .with_context(|| format!("writing {}", path.display()))
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Flag {
    pub long: Option<String>,
    pub short: Option<char>,
    pub takes_value: bool,
}

fn package_name(name: &str) -> String {
    let mut out: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    out = out.trim_matches('-').to_string();
    if out.is_empty() || out.starts_with(|c: char| c.is_ascii_digit()) {
        out = format!("app-{out}").trim_end_matches('-').to_string();
    }
    out
}

pub(crate) fn ident(name: &str) -> String {
    let mut out: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    let keywords = [
        "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn",
        "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref",
        "return", "self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use",
        "where", "while", "async", "await", "dyn", "gen", "try",
    ];
    if out.is_empty()
        || out.starts_with(|c: char| c.is_ascii_digit())
        || keywords.contains(&out.as_str())
    {
        out = format!("unit_{out}");
    }
    out
}

/// The commands (first arguments of cases the original ran successfully) and the flags seen with
/// each; the key `""` holds the flags of an app without commands.
fn infer_cli(
    cases: &[(Vec<String>, bool)],
) -> (Vec<String>, BTreeMap<String, BTreeMap<String, Flag>>) {
    let commands: BTreeSet<String> = cases
        .iter()
        .filter(|(args, ok)| *ok && args.first().is_some_and(|a| app::is_word(a)))
        .map(|(args, _)| args[0].clone())
        .collect();
    let mut flags: BTreeMap<String, BTreeMap<String, Flag>> = BTreeMap::new();
    for (args, ok) in cases {
        if !ok {
            continue;
        }
        let (key, rest) = match args.split_first() {
            Some((first, rest)) if commands.contains(first) => (first.clone(), rest),
            _ if commands.is_empty() => (String::new(), args.as_slice()),
            _ => continue,
        };
        let entry = flags.entry(key).or_default();
        for arg in rest {
            if arg == "--" {
                break;
            }
            if let Some(long) = arg.strip_prefix("--") {
                let (name, has_value) = match long.split_once('=') {
                    Some((n, _)) => (n, true),
                    None => (long, false),
                };
                let valid = !name.is_empty()
                    && name
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
                if valid {
                    let id = if name == "args" { "flag-args" } else { name };
                    let flag = entry.entry(id.to_string()).or_default();
                    flag.long = Some(name.to_string());
                    flag.takes_value |= has_value;
                }
            } else if let Some(short) = arg.strip_prefix('-') {
                let mut chars = short.chars();
                if let (Some(c), None) = (chars.next(), chars.next())
                    && c.is_ascii_alphabetic()
                {
                    entry.entry(format!("short-{c}")).or_default().short = Some(c);
                }
            }
        }
    }
    (commands.into_iter().collect(), flags)
}

fn arg_lines(flags: Option<&BTreeMap<String, Flag>>, indent: &str) -> String {
    let mut out = String::new();
    for (id, flag) in flags.into_iter().flatten() {
        let mut line = format!("{indent}.arg(Arg::new({id:?})");
        if let Some(long) = &flag.long {
            line += &format!(".long({long:?})");
        }
        if let Some(short) = flag.short {
            line += &format!(".short({short:?})");
        }
        line += if flag.takes_value {
            ".num_args(1).action(ArgAction::Append).value_parser(value_parser!(OsString)))"
        } else {
            ".action(ArgAction::Count))"
        };
        out += &line;
        out.push('\n');
    }
    out += &format!("{indent}.arg(rest())\n");
    out
}

fn cli_rs(
    name: &str,
    commands: &[String],
    flags: &BTreeMap<String, BTreeMap<String, Flag>>,
) -> String {
    let any_flags = flags.values().any(|f| !f.is_empty());
    let imports = if any_flags {
        "Arg, ArgAction, Command, value_parser"
    } else {
        "Arg, Command, value_parser"
    };
    let mut out = format!(
        "use std::ffi::OsString;\n\n\
         use clap::{{{imports}}};\n\n\
         /// The command `units::dispatch` gets when argv names no subcommand.\n\
         pub const ROOT: &str = {name:?};\n\n\
         fn rest() -> Arg {{\n    \
             Arg::new(\"args\")\n        \
                 .num_args(0..)\n        \
                 .trailing_var_arg(true)\n        \
                 .allow_hyphen_values(true)\n        \
                 .value_parser(value_parser!(OsString))\n\
         }}\n\n\
         /// Routes argv to a command. Anything this parser rejects runs the original, so it never\n\
         /// has to match every way the original accepts its arguments.\n\
         pub fn command() -> Command {{\n    \
             Command::new(ROOT)\n        \
                 .disable_help_flag(true)\n        \
                 .disable_version_flag(true)\n"
    );
    if commands.is_empty() {
        out += &arg_lines(flags.get(""), "        ");
    } else {
        out += "        .disable_help_subcommand(true)\n        .args_conflicts_with_subcommands(true)\n        .arg(rest())\n";
        for command in commands {
            out += &format!(
                "        .subcommand(\n            Command::new({command:?})\n                .disable_help_flag(true)\n"
            );
            out += &arg_lines(flags.get(command), "                ");
            out += "        )\n";
        }
    }
    out.truncate(out.trim_end().len());
    out += "\n}\n";
    out
}

pub(crate) const MAIN_RS: &str = "mod cli;
mod original;
mod units;

use std::ffi::OsString;
use std::process::ExitCode;

fn main() -> ExitCode {
    original::report_fallbacks();
    let argv: Vec<OsString> = std::env::args_os().collect();
    if let Ok(matches) = cli::command().try_get_matches_from(&argv) {
        let (command, sub) = matches.subcommand().unwrap_or((cli::ROOT, &matches));
        if let Some(code) = units::dispatch(command, sub) {
            return code;
        }
    }
    original::run(&argv[1..])
}
";

fn original_rs(run: &[String]) -> String {
    let words: Vec<String> = run.iter().map(|w| format!("{w:?}")).collect();
    format!(
        "use std::ffi::OsString;
use std::io::Write;
use std::path::Path;
use std::process::{{Command, ExitCode, ExitStatus}};

const ROOT: &str = concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/../..\");
const RUN: &[&str] = &[{}];
const FALLBACK_LOG: &str = \"{}\";

fn resolve(word: &str) -> OsString {{
    if word.starts_with('-') {{
        return word.into();
    }}
    let path = Path::new(ROOT).join(word);
    let looks_like_path = word.contains('/') || word.contains('\\\\') || path.is_file();
    if looks_like_path && path.exists() {{
        return path.canonicalize().unwrap_or(path).into_os_string();
    }}
    word.into()
}}

fn log() -> Option<std::fs::File> {{
    let path = std::env::var_os(FALLBACK_LOG)?;
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .ok()
}}

/// Tells `rust-anything app replay` this build counts its fallbacks, even when it makes none.
pub fn report_fallbacks() {{
    let _ = log();
}}

/// Runs the original with the same arguments, stdin and environment, and passes its output and
/// exit code through. Each call is one fallback.
pub fn run(args: &[OsString]) -> ExitCode {{
    if let Some(mut file) = log() {{
        let line: Vec<String> = args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        let _ = writeln!(file, \"{{}}\", line.join(\" \"));
    }}
    let Some((program, rest)) = RUN.split_first() else {{
        return ExitCode::from(127);
    }};
    let status = Command::new(resolve(program))
        .args(rest.iter().map(|w| resolve(w)))
        .args(args)
        .status();
    match status {{
        Ok(status) => ExitCode::from(exit_code(status)),
        Err(e) => {{
            eprintln!(\"front door: cannot run the original `{{}}`: {{e}}\", RUN.join(\" \"));
            ExitCode::from(127)
        }}
    }}
}}

fn exit_code(status: ExitStatus) -> u8 {{
    if let Some(code) = status.code() {{
        return (code & 0xff) as u8;
    }}
    #[cfg(unix)]
    {{
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {{
            return (128 + signal) as u8;
        }}
    }}
    1
}}
",
        words.join(", "),
        app::FALLBACK_ENV
    )
}

fn units_rs(state: &State) -> String {
    let mut out = format!(
        "//! Generated by `rust-anything app wire` from {STATE_FILE}; it is rewritten on every wire.\n\n\
         use std::process::ExitCode;\n\n\
         use clap::ArgMatches;\n\n"
    );
    if state.units.is_empty() {
        out += "pub fn dispatch(_command: &str, _matches: &ArgMatches) -> Option<ExitCode> {\n    None\n}\n";
        return out;
    }
    for unit in &state.units {
        out += &format!("mod {};\n", ident(&unit.name));
    }
    out += &format!(
        "\n/// `{KILL_SWITCH_PREFIX}<NAME>=original` sends that unit back to the original.\n\
         fn sent_to_original(unit: &str) -> bool {{\n    \
             std::env::var(format!(\"{KILL_SWITCH_PREFIX}{{unit}}\")).is_ok_and(|v| v == \"original\")\n\
         }}\n\n\
         pub fn dispatch(command: &str, matches: &ArgMatches) -> Option<ExitCode> {{\n    \
             match command {{\n"
    );
    for unit in &state.units {
        let pattern: Vec<String> = unit.commands.iter().map(|c| format!("{c:?}")).collect();
        let switch = unit
            .switch
            .strip_prefix(KILL_SWITCH_PREFIX)
            .unwrap_or(&unit.switch);
        out += &format!(
            "        {} if !sent_to_original({switch:?}) => Some({}::run(command, matches)),\n",
            pattern.join(" | "),
            ident(&unit.name)
        );
    }
    out += "        _ => None,\n    }\n}\n";
    out
}

fn glue_rs(crate_name: &str) -> String {
    let krate = crate_name.replace('-', "_");
    format!(
        "//! Glue from the front door to `{crate_name}`: parse the arguments, call `{krate}::port`,\n\
         //! print what the original prints and return its exit code.\n\n\
         use std::ffi::OsString;\n\
         use std::process::ExitCode;\n\n\
         use clap::ArgMatches;\n\n\
         pub fn run(command: &str, matches: &ArgMatches) -> ExitCode {{\n    \
             let args: Vec<&OsString> = matches\n        \
                 .get_many::<OsString>(\"args\")\n        \
                 .into_iter()\n        \
                 .flatten()\n        \
                 .collect();\n    \
             unimplemented!(\"glue for `{{command}}` {{args:?}}: call {krate}::port\")\n\
         }}\n"
    )
}

pub(crate) fn rustfmt(file: &Path) {
    let _ = Command::new("rustfmt")
        .args(["--edition", "2024"])
        .arg(file)
        .output();
}

pub(crate) fn cargo_build(app_dir: &Path, target_dir: Option<&Path>) -> Result<PathBuf> {
    let mut cmd = Command::new("cargo");
    cmd.args([
        "build",
        "--message-format=json-render-diagnostics",
        "--manifest-path",
    ])
    .arg(app_dir.join("Cargo.toml"));
    if let Some(dir) = target_dir {
        cmd.env("CARGO_TARGET_DIR", dir);
    }
    let out = cmd.output().context("running cargo")?;
    ensure!(
        out.status.success(),
        "cargo build in {} failed:\n{}",
        app_dir.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    let mut exe = None;
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let Ok(msg) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let is_bin = msg["target"]["kind"]
            .as_array()
            .is_some_and(|k| k.iter().any(|v| v == "bin"));
        if msg["reason"] == "compiler-artifact"
            && is_bin
            && let Some(path) = msg["executable"].as_str()
        {
            exe = Some(PathBuf::from(path));
        }
    }
    exe.context("cargo built no binary")
}

fn build_and_replay(repo: &Path) -> Result<ReplayOutcome> {
    let exe = cargo_build(&repo.join(APP_DIR), None)?;
    app::replay(&ReplayOptions {
        repo,
        build: Some(&app::shell_quote(&exe.to_string_lossy())),
        timeout_secs: None,
    })
}

pub struct FrontDoorOptions<'a> {
    pub repo: &'a Path,
    /// Build the front door and replay the recorded cases against it.
    pub check: bool,
}

#[derive(Debug, Serialize)]
pub struct FrontDoorOutcome {
    pub dir: PathBuf,
    pub binary: String,
    pub commands: Vec<String>,
    pub flags: BTreeMap<String, BTreeMap<String, Flag>>,
    pub files: Vec<String>,
    pub replay: Option<ReplayOutcome>,
}

pub fn front_door(opts: &FrontDoorOptions) -> Result<FrontDoorOutcome> {
    let repo = opts
        .repo
        .canonicalize()
        .with_context(|| format!("no folder {}", opts.repo.display()))?;
    let config = app::Config::load(&repo)?.with_context(|| {
        format!(
            "no recorded cases in {}: run `rust-anything app record` first",
            app::cases_dir(&repo).display()
        )
    })?;
    let dir = repo.join(APP_DIR);
    ensure!(
        !dir.exists(),
        "{} already exists; the front door is yours once scaffolded (remove it to scaffold again)",
        dir.display()
    );
    let (commands, flags) = infer_cli(&app::recorded_args(&repo)?);
    let binary = package_name(&config.name);
    let files: Vec<(&str, String)> = vec![
        (
            "Cargo.toml",
            format!(
                "[package]\nname = \"{binary}\"\nversion = \"0.1.0\"\nedition = \"2024\"\npublish = false\n\
                 description = \"Front door for `{}`, generated by rust-anything: Rust owns the entry point, the original answers what is not ported yet\"\n\n\
                 [[bin]]\nname = \"{binary}\"\npath = \"src/main.rs\"\n\n\
                 [dependencies]\nclap = \"4\"\n\n\
                 [workspace]\n",
                config.name
            ),
        ),
        (".gitignore", "/target\n".to_string()),
        ("src/main.rs", MAIN_RS.to_string()),
        ("src/cli.rs", cli_rs(&config.name, &commands, &flags)),
        ("src/original.rs", original_rs(&config.run)),
    ];
    let state = State {
        schema: SCHEMA.to_string(),
        name: config.name.clone(),
        commands: commands.clone(),
        units: Vec::new(),
        lifted: false,
    };
    for (path, text) in &files {
        let target = dir.join(path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&target, text).with_context(|| format!("writing {}", target.display()))?;
    }
    std::fs::create_dir_all(dir.join("src/units"))?;
    std::fs::write(dir.join("src/units/mod.rs"), units_rs(&state))?;
    rustfmt(&dir.join("src/main.rs"));
    state.save(&repo)?;
    let replay = if opts.check {
        Some(build_and_replay(&repo)?)
    } else {
        None
    };
    let mut written: Vec<String> = files.iter().map(|(p, _)| p.to_string()).collect();
    written.extend(["src/units/mod.rs".to_string(), STATE_FILE.to_string()]);
    Ok(FrontDoorOutcome {
        dir,
        binary,
        commands,
        flags,
        files: written,
        replay,
    })
}

fn find_unit(repo: &Path, unit: &str) -> Result<PathBuf> {
    let rust = repo.join("rust");
    for candidate in [
        rust.join(unit),
        rust.join(format!("{unit}-rs")),
        repo.join(unit),
        PathBuf::from(unit),
    ] {
        if candidate.join("Cargo.toml").is_file() && candidate != repo.join(APP_DIR) {
            return Ok(candidate.canonicalize()?);
        }
    }
    let wanted = unit.trim_end_matches('/');
    if let Ok(entries) = std::fs::read_dir(&rust) {
        for entry in entries.flatten() {
            if let Ok(m) = crate::bridge::Manifest::load(&entry.path())
                && m.unit.trim_end_matches('/') == wanted
            {
                return Ok(entry.path().canonicalize()?);
            }
        }
    }
    bail!(
        "no unit crate `{unit}`: give the crate folder (`rust/<name>`, as `bridge` made it) or the unit id"
    )
}

pub struct WireOptions<'a> {
    pub repo: &'a Path,
    /// Unit crate folder, its name under `rust/`, or the unit id it was bridged from.
    pub unit: &'a str,
    /// Commands the unit answers; kept from the last wire when empty.
    pub commands: &'a [String],
    /// Wire without running `gate` on the unit first (when it just passed).
    pub skip_gate: bool,
    pub check: bool,
}

#[derive(Debug, Serialize)]
pub struct WireOutcome {
    pub unit: String,
    pub crate_dir: PathBuf,
    pub commands: Vec<String>,
    pub switch: String,
    pub glue: PathBuf,
    /// The glue file still holds the generated stub.
    pub glue_pending: bool,
    pub gate: Option<bool>,
    pub replay: Option<ReplayOutcome>,
}

pub fn wire(opts: &WireOptions) -> Result<WireOutcome> {
    let repo = opts
        .repo
        .canonicalize()
        .with_context(|| format!("no folder {}", opts.repo.display()))?;
    let mut state = State::load(&repo)?;
    ensure!(
        !state.lifted,
        "the app is lifted: there is no fallback left to wire around; call the crate from src/units directly"
    );
    let app_dir = repo.join(APP_DIR);
    let crate_dir = find_unit(&repo, opts.unit)?;
    let manifest_text = std::fs::read_to_string(crate_dir.join("Cargo.toml"))?;
    let unit_doc: DocumentMut = manifest_text.parse()?;
    let crate_name = unit_doc
        .get("package")
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .context("the unit's Cargo.toml has no package name")?
        .to_string();
    if let Some(kinds) = unit_doc
        .get("lib")
        .and_then(|l| l.get("crate-type"))
        .and_then(|k| k.as_array())
    {
        ensure!(
            kinds.iter().any(|k| k.as_str() == Some("rlib")),
            "{crate_name} is not a Rust library: add \"rlib\" to `crate-type` in its Cargo.toml"
        );
    }
    let name = crate_name
        .strip_suffix("-rs")
        .unwrap_or(&crate_name)
        .to_string();
    let switch = format!(
        "{KILL_SWITCH_PREFIX}{}",
        ident(&name).trim_start_matches("unit_").to_uppercase()
    );

    let gate = if opts.skip_gate {
        None
    } else {
        let outcome = crate::gate::gate(&crate_dir, &[])?;
        if !outcome.passed {
            let failed: Vec<String> = outcome
                .checks
                .iter()
                .filter(|c| !c.passed)
                .map(|c| c.name.to_string())
                .collect();
            bail!(
                "{crate_name} does not pass gate ({}): run `rust-anything gate {}` and fix it before wiring",
                failed.join(", "),
                crate_dir.display()
            );
        }
        Some(true)
    };

    let previous = state.units.iter().position(|u| u.name == name);
    let commands = if opts.commands.is_empty() {
        previous
            .map(|i| state.units[i].commands.clone())
            .with_context(|| format!("name the commands {name} answers with --command"))?
    } else {
        opts.commands.to_vec()
    };
    let routable: Vec<&str> = state
        .commands
        .iter()
        .map(String::as_str)
        .chain([state.name.as_str()])
        .collect();
    for command in &commands {
        ensure!(
            routable.contains(&command.as_str()),
            "the front door has no command `{command}` (it has: {}); add it to src/cli.rs and to `commands` in {}",
            routable.join(", "),
            app_dir.join(STATE_FILE).display()
        );
        if let Some(other) = state
            .units
            .iter()
            .find(|u| u.name != name && u.commands.contains(command))
        {
            bail!("`{command}` is already wired to {}", other.name);
        }
    }
    let path = match crate_dir.strip_prefix(repo.join("rust")) {
        Ok(rel) => format!("../{}", rel.to_string_lossy().replace('\\', "/")),
        Err(_) => crate_dir.to_string_lossy().into_owned(),
    };
    let entry = WiredUnit {
        name: name.clone(),
        crate_name: crate_name.clone(),
        path: path.clone(),
        commands: commands.clone(),
        switch: switch.clone(),
    };
    match previous {
        Some(i) => state.units[i] = entry,
        None => state.units.push(entry),
    }

    let cargo_path = app_dir.join("Cargo.toml");
    let mut doc: DocumentMut = std::fs::read_to_string(&cargo_path)?.parse()?;
    let mut dep = InlineTable::new();
    dep.insert("path", path.as_str().into());
    doc["dependencies"][crate_name.as_str()] = value(dep);
    std::fs::write(&cargo_path, doc.to_string())?;

    let glue = app_dir
        .join("src/units")
        .join(format!("{}.rs", ident(&name)));
    if !glue.exists() {
        std::fs::write(&glue, glue_rs(&crate_name))?;
    }
    std::fs::write(app_dir.join("src/units/mod.rs"), units_rs(&state))?;
    rustfmt(&app_dir.join("src/units/mod.rs"));
    state.save(&repo)?;
    let glue_pending = std::fs::read_to_string(&glue)?.contains(GLUE_MARKER);
    let replay = if opts.check {
        Some(build_and_replay(&repo)?)
    } else {
        None
    };
    Ok(WireOutcome {
        unit: name,
        crate_dir,
        commands,
        switch,
        glue,
        glue_pending,
        gate,
        replay,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case(args: &[&str], ok: bool) -> (Vec<String>, bool) {
        (args.iter().map(|a| a.to_string()).collect(), ok)
    }

    #[test]
    fn commands_and_flags_come_from_successful_cases() {
        let (commands, flags) = infer_cli(&[
            case(&["div", "7", "2"], true),
            case(&["show", "--json", "-v", "x"], true),
            case(&["show", "--width=3"], true),
            case(&["nope"], false),
            case(&["--help"], true),
        ]);
        assert_eq!(commands, ["div", "show"]);
        let show = &flags["show"];
        assert!(!show["json"].takes_value);
        assert!(show["width"].takes_value);
        assert_eq!(show["short-v"].short, Some('v'));
        assert!(!flags.contains_key("div") || flags["div"].is_empty());
        let (none, root) = infer_cli(&[case(&["-n", "5", "file.txt"], true)]);
        assert!(none.is_empty());
        assert!(root[""].contains_key("short-n"));
    }

    #[test]
    fn names_are_valid_rust() {
        assert_eq!(package_name("My Tool"), "my-tool");
        assert_eq!(package_name("7zip"), "app-7zip");
        assert_eq!(ident("text-kit"), "text_kit");
        assert_eq!(ident("mod"), "unit_mod");
    }

    #[test]
    fn units_route_commands_behind_kill_switches() {
        let mut state = State {
            schema: SCHEMA.to_string(),
            name: "tcli".to_string(),
            commands: vec!["div".into(), "words".into()],
            units: Vec::new(),
            lifted: false,
        };
        assert!(units_rs(&state).contains("None"));
        state.units.push(WiredUnit {
            name: "wordcount".into(),
            crate_name: "wordcount-rs".into(),
            path: "../wordcount-rs".into(),
            commands: vec!["words".into()],
            switch: "RUST_ANYTHING_UNIT_WORDCOUNT".into(),
        });
        let code = units_rs(&state);
        assert!(code.contains("mod wordcount;"), "{code}");
        assert!(
            code.contains(
                "\"words\" if !sent_to_original(\"WORDCOUNT\") => Some(wordcount::run(command, matches)),"
            ),
            "{code}"
        );
    }
}
