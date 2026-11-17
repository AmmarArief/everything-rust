//! Source languages. TypeScript/JavaScript lives in `crate::ts`; the others are plugins that
//! produce the same workspace, facts and exports, with types mapped into the TS-like type
//! dialect the planner, generators and port stubs use (`string`, `number`, `int`, `T[]`, ...).

use std::path::Path;

use serde::{Deserialize, Serialize};

pub mod c;
pub mod go;
pub mod java;
pub mod php;
pub mod python;
pub mod ruby;
pub mod syntax;

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, clap::ValueEnum,
)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    #[default]
    Ts,
    Python,
    C,
    Go,
    Ruby,
    Java,
    Php,
}

impl Lang {
    pub fn name(self) -> &'static str {
        match self {
            Self::Ts => "TypeScript/JavaScript",
            Self::Python => "Python",
            Self::C => "C",
            Self::Go => "Go",
            Self::Ruby => "Ruby",
            Self::Java => "Java",
            Self::Php => "PHP",
        }
    }

    /// The language of a repository: from its manifest files, else its most common source files.
    pub fn detect(root: &Path) -> Self {
        let has = |name: &str| root.join(name).exists();
        if has("go.mod") {
            return Self::Go;
        }
        if has("pom.xml") || has("build.gradle") || has("build.gradle.kts") {
            return Self::Java;
        }
        if has("pyproject.toml") || has("setup.py") || has("setup.cfg") {
            return Self::Python;
        }
        if has("Gemfile") {
            return Self::Ruby;
        }
        if has("composer.json") {
            return Self::Php;
        }
        if has("package.json") {
            return Self::Ts;
        }
        let mut counts = [0usize; 7];
        for entry in crate::walk::builder(root)
            .hidden(true)
            .require_git(false)
            .build()
            .flatten()
        {
            let ext = entry
                .path()
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");
            if !entry.file_type().is_some_and(|kind| kind.is_file()) {
                continue;
            }
            if let Some(i) = Self::ALL.iter().position(|l| l.extensions().contains(&ext)) {
                counts[i] += 1;
            }
        }
        let best = (0..7)
            .max_by_key(|&i| (counts[i], std::cmp::Reverse(i)))
            .unwrap_or(0);
        Self::ALL[best]
    }

    const ALL: [Self; 7] = [
        Self::Ts,
        Self::Python,
        Self::C,
        Self::Go,
        Self::Ruby,
        Self::Java,
        Self::Php,
    ];

    /// File extensions of the language's source files.
    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            Self::Ts => &[
                "ts", "mts", "cts", "js", "mjs", "cjs", "tsx", "jsx", "vue", "svelte", "astro",
            ],
            Self::Python => &["py"],
            Self::C => &["c", "h"],
            Self::Go => &["go"],
            Self::Ruby => &["rb"],
            Self::Java => &["java"],
            Self::Php => &["php"],
        }
    }
}

/// Where an import points, for languages other than TS.
pub enum Resolution {
    File(usize),
    External(String),
    Stdlib,
    Unresolved,
}

pub fn discover(lang: Lang, root: &Path) -> anyhow::Result<crate::ts::discover::Workspace> {
    match lang {
        Lang::Python => python::discover(root),
        Lang::C => c::discover(root),
        Lang::Go => go::discover(root),
        Lang::Ruby => ruby::discover(root),
        Lang::Java => java::discover(root),
        Lang::Php => php::discover(root),
        _ => anyhow::bail!("{} is not supported yet", lang.name()),
    }
}

pub fn scan(lang: Lang, _path: &str, text: &str) -> crate::ts::scan::FileFacts {
    match lang {
        Lang::Python => python::scan(text),
        Lang::C => c::scan(text),
        Lang::Go => go::scan(text),
        Lang::Ruby => ruby::scan(text),
        Lang::Java => java::scan(text),
        Lang::Php => php::scan(text),
        _ => crate::ts::scan::FileFacts::default(),
    }
}

pub fn resolve(
    lang: Lang,
    from: &str,
    spec: &str,
    index: &std::collections::HashMap<&str, usize>,
    ws: &crate::ts::discover::Workspace,
) -> Resolution {
    match lang {
        Lang::Python => match python::resolve(from, spec, index, ws) {
            python::Resolution::File(i) => Resolution::File(i),
            python::Resolution::External(n) => Resolution::External(n),
            python::Resolution::Stdlib => Resolution::Stdlib,
            python::Resolution::Unresolved => Resolution::Unresolved,
        },
        Lang::C => c::resolve(from, spec, index),
        Lang::Go => go::resolve(spec, index, ws),
        Lang::Ruby => ruby::resolve(from, spec, index, ws),
        Lang::Java => java::resolve(spec, index, ws),
        Lang::Php => php::resolve(from, spec, index, ws),
        _ => Resolution::Unresolved,
    }
}

pub fn exports(
    lang: Lang,
    path: &str,
    text: &str,
) -> (
    Vec<crate::ts::exports::ExportedFn>,
    Vec<crate::ts::exports::ExportedClass>,
) {
    match lang {
        Lang::Python => python::exports(path, text),
        Lang::C => (c::exports(path, text), Vec::new()),
        Lang::Go => (go::exports(path, text), Vec::new()),
        Lang::Ruby => (ruby::exports(path, text), Vec::new()),
        Lang::Java => (java::exports(path, text), Vec::new()),
        Lang::Php => (php::exports(path, text), Vec::new()),
        _ => (Vec::new(), Vec::new()),
    }
}

/// Dependency tables of the languages without their own module entry yet.
pub fn lookup(lang: Lang, name: &str) -> Option<crate::ts::deps::CrateMatch> {
    match lang {
        Lang::C => c::lookup(name),
        Lang::Go => go::lookup(name),
        Lang::Ruby => ruby::lookup(name),
        Lang::Java => java::lookup(name),
        Lang::Php => php::lookup(name),
        _ => None,
    }
}
