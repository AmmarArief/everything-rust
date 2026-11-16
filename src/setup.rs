//! `setup`: register rust-anything with the coding agents on this machine, in one command.
//!
//! For each agent it finds (or is told about), it adds the MCP server (`rust-anything mcp`) to
//! the agent's own configuration and, where the agent reads skills, installs the workflow skill.
//! Existing settings are kept: only the `rust-anything` entries are added or replaced, and a file
//! that does not parse is left alone with an error. `--remove` takes the entries out again.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Serialize;
use serde_json::{Value, json};

/// The server and skill name in every agent.
pub const NAME: &str = "rust-anything";

/// The workflow skill, as shipped in `skills/rust-anything/SKILL.md`.
pub const SKILL: &str = include_str!("../skills/rust-anything/SKILL.md");

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Agent {
    /// Claude Code: `~/.claude.json`, skill in `~/.claude/skills`.
    Claude,
    /// OpenAI Codex CLI: `~/.codex/config.toml`, skill in `~/.codex/skills`.
    Codex,
    /// Cursor: `~/.cursor/mcp.json`.
    Cursor,
    /// Gemini CLI: `~/.gemini/settings.json`.
    Gemini,
}

impl Agent {
    pub const ALL: [Agent; 4] = [Agent::Claude, Agent::Codex, Agent::Cursor, Agent::Gemini];

    pub fn name(self) -> &'static str {
        match self {
            Agent::Claude => "Claude Code",
            Agent::Codex => "Codex",
            Agent::Cursor => "Cursor",
            Agent::Gemini => "Gemini CLI",
        }
    }

    /// The agent's folder in the home directory, and its command.
    fn markers(self) -> (&'static str, &'static str) {
        match self {
            Agent::Claude => (".claude", "claude"),
            Agent::Codex => (".codex", "codex"),
            Agent::Cursor => (".cursor", "cursor"),
            Agent::Gemini => (".gemini", "gemini"),
        }
    }

    /// Whether the agent seems installed: its folder exists or its command is on `PATH`.
    fn present(self, home: &Path) -> bool {
        let (dir, command) = self.markers();
        home.join(dir).is_dir() || on_path(command)
    }
}

fn on_path(command: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths).any(|dir| {
            let file = dir.join(command);
            file.is_file()
                || file.with_extension("exe").is_file()
                || file.with_extension("cmd").is_file()
        })
    })
}

pub struct Options<'a> {
    pub home: &'a Path,
    /// Agents to set up; empty means every agent found.
    pub agents: Vec<Agent>,
    /// The command the agents run; the current executable by default.
    pub command: PathBuf,
    pub remove: bool,
    pub dry_run: bool,
}

/// One change: which agent, which file, and what happened.
#[derive(Debug, Serialize)]
pub struct Change {
    pub agent: Agent,
    pub file: PathBuf,
    /// `added`, `updated`, `unchanged`, `removed` or `absent`.
    pub action: &'static str,
    /// The copy of the file as it was, when an existing file was changed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub command: PathBuf,
    pub agents: Vec<Agent>,
    pub changes: Vec<Change>,
    pub dry_run: bool,
}

/// The home directory: `HOME`, or `USERPROFILE` on Windows.
pub fn home() -> Result<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .context("no home directory (HOME is not set)")
}

pub fn setup(opts: &Options) -> Result<Report> {
    let agents: Vec<Agent> = if opts.agents.is_empty() {
        Agent::ALL
            .into_iter()
            .filter(|a| a.present(opts.home))
            .collect()
    } else {
        opts.agents.clone()
    };
    if agents.is_empty() {
        bail!(
            "no coding agent found (looked for ~/.claude, ~/.codex, ~/.cursor, ~/.gemini and their commands); \
             name one with --agent claude|codex|cursor|gemini"
        );
    }
    let command = opts.command.to_string_lossy().into_owned();
    let mut changes = Vec::new();
    for &agent in &agents {
        let home = opts.home;
        match agent {
            Agent::Claude => {
                changes.push(json_server(
                    agent,
                    &home.join(".claude.json"),
                    &command,
                    opts,
                    true,
                )?);
                changes.push(skill(agent, &home.join(".claude/skills").join(NAME), opts)?);
            }
            Agent::Codex => {
                changes.push(codex_server(
                    agent,
                    &home.join(".codex/config.toml"),
                    &command,
                    opts,
                )?);
                changes.push(skill(agent, &home.join(".codex/skills").join(NAME), opts)?);
            }
            Agent::Cursor => {
                changes.push(json_server(
                    agent,
                    &home.join(".cursor/mcp.json"),
                    &command,
                    opts,
                    false,
                )?);
            }
            Agent::Gemini => {
                changes.push(json_server(
                    agent,
                    &home.join(".gemini/settings.json"),
                    &command,
                    opts,
                    false,
                )?);
            }
        }
    }
    Ok(Report {
        command: opts.command.clone(),
        agents,
        changes,
        dry_run: opts.dry_run,
    })
}

/// Copies an existing settings file to `<file>.rust-anything.bak` before it is changed.
fn backup(path: &Path) -> Result<Option<PathBuf>> {
    if !path.is_file() {
        return Ok(None);
    }
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".rust-anything.bak");
    let copy = path.with_file_name(name);
    std::fs::copy(path, &copy).with_context(|| format!("backing up {}", path.display()))?;
    Ok(Some(copy))
}

/// Writes `text` to `path` through a temporary file, so a failed write never leaves half a file.
fn write(path: &Path, text: &str) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    let tmp = path.with_extension("rust-anything.tmp");
    std::fs::write(&tmp, text).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("replacing {}", path.display()))
}

/// The `mcpServers` entry of a JSON settings file (Claude Code, Cursor, Gemini CLI).
fn json_server(
    agent: Agent,
    path: &Path,
    command: &str,
    opts: &Options,
    stdio_type: bool,
) -> Result<Change> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => Some(t),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
    };
    let mut doc: Value = match text.as_deref().map(str::trim) {
        None | Some("") => json!({}),
        Some(t) => serde_json::from_str(t).with_context(|| {
            format!(
                "{} is not valid JSON; fix it or add the server by hand",
                path.display()
            )
        })?,
    };
    let Some(root) = doc.as_object_mut() else {
        bail!(
            "{} is not a JSON object; add the server by hand",
            path.display()
        );
    };
    let mut entry = json!({ "command": command, "args": ["mcp"] });
    if stdio_type {
        entry = json!({ "type": "stdio", "command": command, "args": ["mcp"], "env": {} });
    }
    let servers = root.entry("mcpServers").or_insert_with(|| json!({}));
    let Some(servers) = servers.as_object_mut() else {
        bail!(
            "`mcpServers` in {} is not an object; add the server by hand",
            path.display()
        );
    };
    let action = if opts.remove {
        if servers.shift_remove(NAME).is_some() {
            "removed"
        } else {
            "absent"
        }
    } else {
        match servers.get(NAME) {
            Some(old) if *old == entry => "unchanged",
            Some(_) => {
                servers.insert(NAME.into(), entry);
                "updated"
            }
            None => {
                servers.insert(NAME.into(), entry);
                "added"
            }
        }
    };
    let mut saved = None;
    if !opts.dry_run && matches!(action, "added" | "updated" | "removed") {
        saved = backup(path)?;
        write(path, &(serde_json::to_string_pretty(&doc)? + "\n"))?;
    }
    Ok(Change {
        agent,
        file: path.to_path_buf(),
        action,
        backup: saved,
    })
}

/// `[mcp_servers.rust-anything]` in Codex's `config.toml`, keeping the rest of the file as is.
fn codex_server(agent: Agent, path: &Path, command: &str, opts: &Options) -> Result<Change> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
    };
    let mut doc: toml_edit::DocumentMut = text.parse().with_context(|| {
        format!(
            "{} is not valid TOML; fix it or add the server by hand",
            path.display()
        )
    })?;
    let servers = doc
        .entry("mcp_servers")
        .or_insert_with(|| {
            let mut t = toml_edit::Table::new();
            t.set_implicit(true);
            toml_edit::Item::Table(t)
        })
        .as_table_mut()
        .with_context(|| format!("`mcp_servers` in {} is not a table", path.display()))?;
    let mut entry = toml_edit::Table::new();
    entry["command"] = toml_edit::value(command);
    let mut args = toml_edit::Array::new();
    args.push("mcp");
    entry["args"] = toml_edit::value(args);
    let same = servers
        .get(NAME)
        .and_then(|t| t.as_table())
        .is_some_and(|t| {
            t.get("command").and_then(|c| c.as_str()) == Some(command)
                && t.get("args").and_then(|a| a.as_array()).is_some_and(|a| {
                    a.iter().map(|v| v.as_str()).collect::<Vec<_>>() == [Some("mcp")]
                })
        });
    let action = if opts.remove {
        if servers.remove(NAME).is_some() {
            "removed"
        } else {
            "absent"
        }
    } else if same {
        "unchanged"
    } else {
        let had = servers.contains_key(NAME);
        servers.insert(NAME, toml_edit::Item::Table(entry));
        if had { "updated" } else { "added" }
    };
    let mut saved = None;
    if !opts.dry_run && matches!(action, "added" | "updated" | "removed") {
        saved = backup(path)?;
        write(path, &doc.to_string())?;
    }
    Ok(Change {
        agent,
        file: path.to_path_buf(),
        action,
        backup: saved,
    })
}

/// The workflow skill in an agent's skills folder.
fn skill(agent: Agent, dir: &Path, opts: &Options) -> Result<Change> {
    let path = dir.join("SKILL.md");
    let current = std::fs::read_to_string(&path).ok();
    let action = if opts.remove {
        if current.is_some() {
            "removed"
        } else {
            "absent"
        }
    } else {
        match current.as_deref() {
            Some(text) if text == SKILL => "unchanged",
            Some(_) => "updated",
            None => "added",
        }
    };
    if !opts.dry_run {
        match action {
            "added" | "updated" => write(&path, SKILL)?,
            "removed" => {
                std::fs::remove_file(&path)
                    .with_context(|| format!("removing {}", path.display()))?;
                // The folder is ours; leave it only if someone put other files in it.
                let _ = std::fs::remove_dir(dir);
            }
            _ => {}
        }
    }
    Ok(Change {
        agent,
        file: path,
        action,
        backup: None,
    })
}

/// The report for people.
pub fn text(r: &Report) -> String {
    let mut out = String::new();
    let verb = if r.dry_run { "Would set up" } else { "Set up" };
    let names: Vec<&str> = r.agents.iter().map(|a| a.name()).collect();
    out.push_str(&format!("{verb} rust-anything for {}\n", names.join(", ")));
    for c in &r.changes {
        out.push_str(&format!(
            "  {:<10} {:<9} {}\n",
            c.agent.name(),
            c.action,
            c.file.display()
        ));
        if let Some(b) = &c.backup {
            out.push_str(&format!("  {:<10} {:<9} {}\n", "", "backup", b.display()));
        }
    }
    if !r.dry_run
        && r.changes
            .iter()
            .any(|c| c.action != "removed" && c.action != "absent")
    {
        out.push_str(&format!(
            "\nThe agents run `{} mcp`. Restart them, then ask, for example:\n  \
             \"Profile this repo's slowest workflow with rust-anything and move the hot loop to Rust.\"\n",
            r.command.display()
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_home(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("rust-anything-setup-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn opts(home: &Path, agents: Vec<Agent>) -> Options<'_> {
        Options {
            home,
            agents,
            command: PathBuf::from("/opt/bin/rust-anything"),
            remove: false,
            dry_run: false,
        }
    }

    #[test]
    fn adds_keeps_and_removes_entries() {
        let home = temp_home("all");
        // Existing settings that must survive, in their own order.
        std::fs::write(
            home.join(".claude.json"),
            r#"{"zeta": 1, "mcpServers": {"other": {"command": "x"}}, "alpha": true}"#,
        )
        .unwrap();
        std::fs::create_dir_all(home.join(".codex")).unwrap();
        std::fs::write(
            home.join(".codex/config.toml"),
            "model = \"o4\"\n\n[mcp_servers.other]\ncommand = \"x\"\n",
        )
        .unwrap();

        let all = Agent::ALL.to_vec();
        let r = setup(&opts(&home, all.clone())).unwrap();
        assert!(
            r.changes.iter().all(|c| c.action == "added"),
            "{:?}",
            r.changes
        );

        let claude: Value =
            serde_json::from_str(&std::fs::read_to_string(home.join(".claude.json")).unwrap())
                .unwrap();
        assert_eq!(
            claude["mcpServers"]["rust-anything"]["args"],
            json!(["mcp"])
        );
        assert_eq!(claude["mcpServers"]["other"]["command"], "x");
        // The existing file was copied before the change; new files need no copy.
        let saved = r.changes[0]
            .backup
            .clone()
            .expect("an existing file is backed up");
        assert!(
            std::fs::read_to_string(saved)
                .unwrap()
                .starts_with(r#"{"zeta": 1"#)
        );
        assert!(r.changes.iter().any(|c| c.backup.is_none()));
        let keys: Vec<&String> = claude.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["zeta", "mcpServers", "alpha"], "key order kept");
        let codex = std::fs::read_to_string(home.join(".codex/config.toml")).unwrap();
        assert!(codex.starts_with("model = \"o4\""), "{codex}");
        assert!(codex.contains("[mcp_servers.rust-anything]\ncommand = \"/opt/bin/rust-anything\"\nargs = [\"mcp\"]"), "{codex}");
        assert!(codex.contains("[mcp_servers.other]"));
        assert_eq!(
            std::fs::read_to_string(home.join(".claude/skills/rust-anything/SKILL.md")).unwrap(),
            SKILL
        );
        let cursor: Value =
            serde_json::from_str(&std::fs::read_to_string(home.join(".cursor/mcp.json")).unwrap())
                .unwrap();
        assert_eq!(
            cursor["mcpServers"]["rust-anything"]["command"],
            "/opt/bin/rust-anything"
        );
        assert!(home.join(".gemini/settings.json").exists());

        // Running again changes nothing.
        let again = setup(&opts(&home, all.clone())).unwrap();
        assert!(
            again.changes.iter().all(|c| c.action == "unchanged"),
            "{:?}",
            again.changes
        );

        // A moved binary updates the entries.
        let mut moved = opts(&home, vec![Agent::Codex]);
        moved.command = PathBuf::from("/usr/local/bin/rust-anything");
        let r = setup(&moved).unwrap();
        assert_eq!(r.changes[0].action, "updated");

        // Remove takes out only our entries.
        let mut remove = opts(&home, all);
        remove.remove = true;
        setup(&remove).unwrap();
        let claude: Value =
            serde_json::from_str(&std::fs::read_to_string(home.join(".claude.json")).unwrap())
                .unwrap();
        assert!(claude["mcpServers"].get("rust-anything").is_none());
        assert_eq!(claude["mcpServers"]["other"]["command"], "x");
        assert!(!home.join(".claude/skills/rust-anything").exists());
        let codex = std::fs::read_to_string(home.join(".codex/config.toml")).unwrap();
        assert!(
            !codex.contains("rust-anything") && codex.contains("[mcp_servers.other]"),
            "{codex}"
        );
        std::fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn finds_agents_and_never_breaks_a_bad_file() {
        let home = temp_home("detect");
        std::fs::create_dir_all(home.join(".cursor")).unwrap();
        std::fs::write(home.join(".cursor/mcp.json"), "{ not json").unwrap();
        // Only Cursor's folder exists (the commands may be on PATH on a developer machine).
        let found: Vec<Agent> = Agent::ALL
            .into_iter()
            .filter(|a| a.present(&home))
            .collect();
        assert!(found.contains(&Agent::Cursor));
        let err = setup(&opts(&home, vec![Agent::Cursor])).unwrap_err();
        assert!(format!("{err:#}").contains("not valid JSON"), "{err:#}");
        assert_eq!(
            std::fs::read_to_string(home.join(".cursor/mcp.json")).unwrap(),
            "{ not json"
        );

        // A dry run writes nothing.
        let mut dry = opts(&home, vec![Agent::Gemini]);
        dry.dry_run = true;
        let r = setup(&dry).unwrap();
        assert_eq!(r.changes[0].action, "added");
        assert!(!home.join(".gemini").exists());
        std::fs::remove_dir_all(&home).unwrap();
    }
}
