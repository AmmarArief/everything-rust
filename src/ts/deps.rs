//! npm packages and their Rust counterparts.
//!
//! The table is deliberately small and opinionated: it names the crate a port should reach
//! for first. A package missing here is reported as `unknown`, never guessed.

use serde::Serialize;

/// How well a Rust crate covers what the npm package does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Fit {
    /// A maintained crate does the same job.
    Good,
    /// A crate covers the common case; some behavior needs checking or extra work.
    Partial,
    /// No Rust equivalent: keep this part in JS or call it across the bridge.
    None,
}

#[derive(Clone, Copy, Debug)]
pub struct CrateMatch {
    pub crates: &'static str,
    pub fit: Fit,
    pub note: &'static str,
}

const fn m(crates: &'static str, fit: Fit, note: &'static str) -> CrateMatch {
    CrateMatch { crates, fit, note }
}

/// The Rust counterpart of an npm package, if the table knows it.
pub fn lookup(package: &str) -> Option<CrateMatch> {
    let found = match package {
        "yaml" | "js-yaml" => m(
            "saphyr, serde_yaml_ng",
            Fit::Partial,
            "Parsing is covered. No crate rewrites YAML while keeping comments and layout.",
        ),
        "json5" => m("json5", Fit::Good, ""),
        "jsonc-parser" => m(
            "jsonc-parser",
            Fit::Good,
            "dprint's crate, with a CST for edits.",
        ),
        "oxc-parser" | "@babel/parser" | "acorn" | "espree" => m(
            "oxc",
            Fit::Good,
            "oxc-parser is already a binding of the Rust crate.",
        ),
        "typescript" => m(
            "oxc",
            Fit::Partial,
            "Parsing is covered. Type checking has no Rust equivalent.",
        ),
        "valibot" | "zod" | "yup" | "superstruct" => m(
            "serde, garde",
            Fit::Partial,
            "Schemas become types plus validation; error messages will differ.",
        ),
        "ajv" => m("jsonschema", Fit::Good, ""),
        "picomatch" | "minimatch" | "micromatch" => {
            m("globset", Fit::Good, "Check brace and extglob edge cases.")
        }
        "tinyglobby" | "fast-glob" | "globby" | "glob" => m("ignore, globset", Fit::Good, ""),
        "ignore" => m("ignore", Fit::Good, ""),
        "chokidar" => m("notify", Fit::Good, ""),
        "cac" | "commander" | "yargs" | "minimist" | "citty" => m("clap", Fit::Good, ""),
        "vscode-languageserver" => m("tower-lsp-server", Fit::Good, ""),
        "vscode-languageserver-textdocument" => m(
            "lsp-textdocument",
            Fit::Partial,
            "LSP positions are UTF-16 by default.",
        ),
        "vscode-jsonrpc" => m("lsp-server, jsonrpsee", Fit::Partial, ""),
        "jiti" | "tsx" | "esbuild-register" | "unconfig" | "c12" => m(
            "-",
            Fit::None,
            "Loads TS/JS config or plugins at runtime. Keep loading in JS and pass data to Rust.",
        ),
        "@vue/compiler-sfc" | "@vue/compiler-dom" => m(
            "-",
            Fit::None,
            "No Rust Vue compiler. Keep SFC parsing in JS or split blocks before the bridge.",
        ),
        "svelte" => m(
            "-",
            Fit::None,
            "The Svelte compiler is JS only. Keep it in JS.",
        ),
        "semver" => m(
            "node-semver",
            Fit::Good,
            "Use node-semver, not semver: npm range syntax differs from Cargo's.",
        ),
        "pathe" | "upath" => m(
            "std::path, camino",
            Fit::Good,
            "Normalize to `/` separators.",
        ),
        "picocolors" | "chalk" | "kleur" | "colorette" => m("owo-colors, anstream", Fit::Good, ""),
        "ws" => m("tokio-tungstenite", Fit::Good, ""),
        "express" | "fastify" | "h3" | "hono" | "koa" => {
            m("axum", Fit::Partial, "Middleware ports by hand.")
        }
        "fast-xml-parser" | "xml2js" | "sax" => m("quick-xml", Fit::Good, ""),
        "smol-toml" | "toml" | "@iarna/toml" => m("toml", Fit::Good, ""),
        "dotenv" => m("dotenvy", Fit::Good, ""),
        "uuid" | "nanoid" => m("uuid, nanoid", Fit::Good, ""),
        "dayjs" | "date-fns" | "luxon" | "moment" => m("jiff", Fit::Good, ""),
        "diff" | "jsdiff" => m("similar", Fit::Good, ""),
        "lru-cache" | "quick-lru" => m("lru", Fit::Good, ""),
        "p-limit" | "p-queue" => m("tokio::sync::Semaphore", Fit::Good, ""),
        "ofetch" | "axios" | "node-fetch" | "undici" | "ky" => m("reqwest", Fit::Good, ""),
        "pino" | "debug" | "consola" | "winston" => m("tracing", Fit::Good, ""),
        "lodash" | "lodash-es" | "es-toolkit" | "radash" => m("std, itertools", Fit::Good, ""),
        "fuse.js" => m(
            "nucleo, fuzzy-matcher",
            Fit::Partial,
            "Scores and ranking order differ.",
        ),
        "magic-string" => m("string_wizard", Fit::Partial, "Check source map output."),
        "devframe" | "vite" | "nuxt" | "@nuxt/kit" | "vitepress" => {
            m("-", Fit::None, "Dev server or UI framework: keep in JS.")
        }
        _ => return None,
    };
    Some(found)
}

/// The package part of a bare import specifier: `@scope/name/sub` -> `@scope/name`.
pub fn package_name(specifier: &str) -> &str {
    let mut parts = specifier.splitn(3, '/');
    let first = parts.next().unwrap_or(specifier);
    if first.starts_with('@') {
        match parts.next() {
            Some(second) => &specifier[..first.len() + 1 + second.len()],
            None => specifier,
        }
    } else {
        first
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_package_names() {
        assert_eq!(package_name("yaml"), "yaml");
        assert_eq!(package_name("svelte/compiler"), "svelte");
        assert_eq!(package_name("@x18n/core"), "@x18n/core");
        assert_eq!(package_name("@x18n/builtins/presets"), "@x18n/builtins");
    }

    #[test]
    fn knows_common_packages() {
        assert_eq!(lookup("json5").map(|m| m.fit), Some(Fit::Good));
        assert_eq!(lookup("jiti").map(|m| m.fit), Some(Fit::None));
        assert!(lookup("left-pad-but-new").is_none());
    }
}
