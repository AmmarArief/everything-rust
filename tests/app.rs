//! `app record` and `app replay` on the same small CLI written in each of the six languages.
//!
//! Each test copies its fixture to a temp folder, records the original, replays the original
//! (everything matches), then replays a build whose division-by-zero message changed (only those
//! cases fail, with a diff in the report). A test is skipped when its runtime is missing.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use rust_anything::app::{self, RecordOptions, ReplayOptions};
use rust_anything::front_door::{self, FrontDoorOptions, WireOptions};

fn available(program: &str, arg: &str) -> bool {
    Command::new(program)
        .arg(arg)
        .output()
        .is_ok_and(|o| o.status.success())
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap().flatten() {
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
}

fn fixture(lang: &str) -> PathBuf {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let repo = std::env::temp_dir().join(format!(
        "rust-anything-app-test-{lang}-{}-{n}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&repo);
    copy_dir(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/cli")
            .join(lang),
        &repo,
    );
    repo
}

fn sh(repo: &Path, program: &str, args: &[&str]) {
    let out = Command::new(program)
        .args(args)
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{program} {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn add_bug(repo: &Path, source: &str, copy_to: Option<&str>) {
    let text = std::fs::read_to_string(repo.join(source)).unwrap();
    assert!(text.contains("division by zero"));
    std::fs::write(
        repo.join(copy_to.unwrap_or(source)),
        text.replace("division by zero", "divide by zero"),
    )
    .unwrap();
}

fn words(line: &str) -> Vec<String> {
    line.split_whitespace().map(str::to_string).collect()
}

fn record_then_replay(repo: &Path, run: &str, buggy: &str) {
    let outcome = app::record(&RecordOptions {
        repo,
        run: &words(run),
        inputs: None,
        normalize: &[],
        name: None,
        traps: true,
        timeout_secs: None,
    })
    .unwrap();
    assert_eq!(outcome.examples, 7, "{outcome:?}");
    assert!(outcome.traps > 30, "{outcome:?}");
    assert!(outcome.unstable.is_empty(), "{outcome:?}");
    assert!(outcome.timed_out.is_empty(), "{outcome:?}");
    assert_eq!(outcome.noisy, ["example-006-now"], "{outcome:?}");

    let dir = app::cases_dir(repo);
    let read = |name: &str| std::fs::read_to_string(dir.join(name)).unwrap();
    assert_eq!(read("example-006-now.stdout"), "now: [..]\n");
    let div = read("example-003-div.toml");
    assert!(div.contains("args = [\"div\", \"7\", \"2\"]"), "{div}");
    assert!(div.contains("status.code = 0"), "{div}");
    assert_eq!(read("example-003-div.stdout"), "7 / 2 = 3 remainder 1\n");
    assert_eq!(read("example-004-words.stdin"), "one two three\n");
    assert_eq!(read("example-004-words.stdout"), "3\n");
    assert!(dir.join("example-005-copy.in/input.txt").is_file());
    assert!(
        read("example-005-copy.out/out.txt").starts_with("GR"),
        "the copy wrote its output file"
    );
    assert!(read("example-007-nope.toml").contains("status.code = 1"));
    assert!(read("example-007-nope.stderr").starts_with("usage: tcli"));

    let original = app::replay(&ReplayOptions {
        repo,
        build: None,
        timeout_secs: None,
    })
    .unwrap();
    assert_eq!(original.total, outcome.cases);
    assert_eq!(
        original.passed,
        original.total,
        "{}",
        std::fs::read_to_string(&original.report).unwrap()
    );
    assert!(original.fallback.is_none());

    let bug = app::replay(&ReplayOptions {
        repo,
        build: Some(buggy),
        timeout_secs: None,
    })
    .unwrap();
    assert!(!bug.failures.is_empty());
    assert_eq!(bug.passed + bug.failures.len(), bug.total);
    for f in &bug.failures {
        assert_eq!(f.args[0], "div", "{f:?}");
        assert!(f.problems.iter().all(|p| p.field == "stderr"), "{f:?}");
    }
    let report = std::fs::read_to_string(&bug.report).unwrap();
    assert!(report.contains("- error: division by zero"), "{report}");
    assert!(report.contains("+ error: divide by zero"), "{report}");

    front_door_then_wire(repo);
    std::fs::remove_dir_all(repo).unwrap();
}

const WORDCOUNT_PORT: &str = "pub fn count_words(text: &[u8]) -> usize {
    text.split(|b| b.is_ascii_whitespace() || *b == 0x0b)
        .filter(|w| !w.is_empty())
        .count()
}
";

const WORDCOUNT_GLUE: &str = "use std::io::Read;
use std::process::ExitCode;

use clap::ArgMatches;

pub fn run(_command: &str, _matches: &ArgMatches) -> ExitCode {
    let mut input = Vec::new();
    if std::io::stdin().read_to_end(&mut input).is_err() {
        return ExitCode::from(1);
    }
    println!(\"{}\", wordcount_rs::port::count_words(&input));
    ExitCode::SUCCESS
}
";

/// The front door answers everything through the original; then a unit takes over `words`.
fn front_door_then_wire(repo: &Path) {
    let door = front_door::front_door(&FrontDoorOptions { repo, check: true }).unwrap();
    assert_eq!(
        door.commands,
        ["copy", "div", "len", "now", "upper", "words"]
    );
    let replay = door.replay.unwrap();
    assert_eq!(replay.passed, replay.total, "{:?}", replay.failures);
    let fallback = replay.fallback.as_ref().unwrap();
    assert_eq!(fallback.cases, replay.total);
    let app_dir = repo.join(front_door::APP_DIR);
    if available("cargo", "--version") {
        sh(&app_dir, "cargo", &["fmt", "--check"]);
    }

    let unit = repo.join("rust/wordcount-rs");
    std::fs::create_dir_all(unit.join("src")).unwrap();
    std::fs::write(
        unit.join("Cargo.toml"),
        "[package]\nname = \"wordcount-rs\"\nversion = \"0.1.0\"\nedition = \"2024\"\npublish = false\n\n[lib]\ncrate-type = [\"cdylib\", \"rlib\"]\n",
    )
    .unwrap();
    std::fs::write(unit.join("src/lib.rs"), "pub mod port;\n").unwrap();
    std::fs::write(unit.join("src/port.rs"), WORDCOUNT_PORT).unwrap();
    let wire = |commands: &[String]| {
        front_door::wire(&WireOptions {
            repo,
            unit: "wordcount-rs",
            commands,
            skip_gate: true,
            check: true,
        })
        .unwrap()
    };

    let stub = wire(&["words".to_string()]);
    assert!(stub.glue_pending);
    assert_eq!(stub.switch, "RUST_ANYTHING_UNIT_WORDCOUNT");
    let failing = stub.replay.unwrap();
    assert!(!failing.failures.is_empty());
    assert!(
        failing.failures.iter().all(|f| f.args[0] == "words"),
        "{:?}",
        failing.failures
    );

    std::fs::write(&stub.glue, WORDCOUNT_GLUE).unwrap();
    let wired = wire(&[]);
    assert!(!wired.glue_pending);
    assert_eq!(wired.commands, ["words"]);
    let replay = wired.replay.unwrap();
    assert_eq!(
        replay.passed,
        replay.total,
        "{}",
        std::fs::read_to_string(&replay.report).unwrap()
    );
    if available("cargo", "--version") {
        sh(&app_dir, "cargo", &["fmt", "--check"]);
        sh(
            &app_dir,
            "cargo",
            &["clippy", "--all-targets", "--", "-D", "warnings"],
        );
    }
    let fallback = replay.fallback.as_ref().unwrap();
    assert!(fallback.cases < fallback.total, "{fallback:?}");
    assert!(fallback.cases >= fallback.total - 5, "{fallback:?}");

    let exe = &replay.build[0];
    let log = repo.join("fallback.log");
    let run = |switch: Option<&str>| {
        let _ = std::fs::remove_file(&log);
        let mut cmd = Command::new(exe);
        cmd.arg("words")
            .env(rust_anything::app::FALLBACK_ENV, &log)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped());
        if let Some(value) = switch {
            cmd.env("RUST_ANYTHING_UNIT_WORDCOUNT", value);
        }
        let mut child = cmd.spawn().unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(b"one two  three\n")
            .unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(out.status.success());
        assert_eq!(String::from_utf8_lossy(&out.stdout), "3\n");
        std::fs::read_to_string(&log).unwrap()
    };
    assert_eq!(run(None), "", "Rust answered");
    assert_eq!(
        run(Some("original")),
        "words\n",
        "the kill switch fell back"
    );
}

#[test]
fn ts_cli_records_and_replays() {
    if !available("node", "--version") {
        eprintln!("skipped: node not found");
        return;
    }
    let repo = fixture("ts");
    add_bug(&repo, "tcli.js", Some("tcli-bug.js"));
    record_then_replay(&repo, "node tcli.js", "node tcli-bug.js");
}

#[test]
fn python_cli_records_and_replays() {
    if !available("python3", "--version") {
        eprintln!("skipped: python3 not found");
        return;
    }
    let repo = fixture("python");
    add_bug(&repo, "tcli.py", Some("tcli_bug.py"));
    record_then_replay(&repo, "python3 tcli.py", "python3 tcli_bug.py");
}

#[test]
fn c_cli_records_and_replays() {
    if !available("cc", "--version") {
        eprintln!("skipped: cc not found");
        return;
    }
    let repo = fixture("c");
    sh(&repo, "cc", &["-O1", "-o", "tcli", "tcli.c"]);
    add_bug(&repo, "tcli.c", Some("tcli-bug.c"));
    sh(&repo, "cc", &["-O1", "-o", "tcli-bug", "tcli-bug.c"]);
    record_then_replay(&repo, "./tcli", "./tcli-bug");
}

#[test]
fn go_cli_records_and_replays() {
    if !available("go", "version") {
        eprintln!("skipped: go not found");
        return;
    }
    let repo = fixture("go");
    sh(&repo, "go", &["build", "-o", "tcli", "."]);
    add_bug(&repo, "main.go", None);
    sh(&repo, "go", &["build", "-o", "tcli-bug", "."]);
    record_then_replay(&repo, "./tcli", "./tcli-bug");
}

#[test]
fn ruby_cli_records_and_replays() {
    if !available("ruby", "--version") {
        eprintln!("skipped: ruby not found");
        return;
    }
    let repo = fixture("ruby");
    add_bug(&repo, "tcli.rb", Some("tcli_bug.rb"));
    record_then_replay(&repo, "ruby tcli.rb", "ruby tcli_bug.rb");
}

#[test]
fn java_cli_records_and_replays() {
    if !available("javac", "--version") || !available("java", "--version") {
        eprintln!("skipped: javac or java not found");
        return;
    }
    let repo = fixture("java");
    sh(&repo, "javac", &["-d", "classes", "Tcli.java"]);
    add_bug(&repo, "Tcli.java", None);
    sh(&repo, "javac", &["-d", "classes-bug", "Tcli.java"]);
    record_then_replay(
        &repo,
        "java -cp ./classes Tcli",
        "java -cp ./classes-bug Tcli",
    );
}

#[test]
fn the_command_exits_non_zero_on_a_mismatch() {
    if !available("python3", "--version") {
        eprintln!("skipped: python3 not found");
        return;
    }
    let repo = fixture("python");
    add_bug(&repo, "tcli.py", Some("tcli_bug.py"));
    std::fs::write(
        repo.join("inputs.jsonl"),
        "{\"name\": \"stamped\", \"args\": [\"upper\", \"at 2026-11-16T10:00:00Z\"]}\n\
         {\"args\": [\"copy\", \"a.txt\", \"b.txt\"], \"files\": {\"a.txt\": \"x\"}}\n\
         {\"args\": [\"div\", \"1\", \"0\"]}\n",
    )
    .unwrap();
    let bin = env!("CARGO_BIN_EXE_rust-anything");
    let record = Command::new(bin)
        .args(["app", "record"])
        .arg(&repo)
        .args([
            "--no-traps",
            "--inputs",
            "inputs.jsonl",
            "--normalize",
            "timestamps",
            "--",
            "python3",
            "tcli.py",
        ])
        .current_dir(&repo)
        .output()
        .unwrap();
    assert!(
        record.status.success(),
        "{}",
        String::from_utf8_lossy(&record.stderr)
    );
    let dir = app::cases_dir(&repo);
    assert_eq!(
        std::fs::read_to_string(dir.join("stamped.stdout")).unwrap(),
        "AT [..]\n"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("input-002-copy.out/b.txt")).unwrap(),
        "X"
    );
    let config = std::fs::read_to_string(dir.join(app::CONFIG_FILE)).unwrap();
    assert!(config.contains("\"timestamps\""), "{config}");

    let replay = |build: Option<&str>| {
        let mut cmd = Command::new(bin);
        cmd.args(["app", "replay"]).arg(&repo);
        if let Some(b) = build {
            cmd.args(["--build", b]);
        }
        cmd.output().unwrap()
    };
    let good = replay(None);
    assert!(good.status.success());
    assert!(String::from_utf8_lossy(&good.stdout).starts_with("10/10 cases passed"));
    let bad = replay(Some("python3 tcli_bug.py"));
    assert_eq!(bad.status.code(), Some(1));
    let text = String::from_utf8_lossy(&bad.stdout);
    assert!(text.starts_with("9/10 cases passed"), "{text}");
    assert!(text.contains("- input-003-div: stderr"), "{text}");
    std::fs::remove_dir_all(&repo).unwrap();
}
