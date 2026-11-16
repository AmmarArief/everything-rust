//! MCP server over stdio: every step as a tool, for any agent.
//!
//! Newline-delimited JSON-RPC 2.0, as the MCP stdio transport specifies. The protocol surface
//! is small (initialize, tools/list, tools/call, ping), so it is implemented directly on
//! serde_json instead of pulling in an SDK.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::{Value, json};

use crate::{analyze, bridge, gate, harness, shadow};

const PROTOCOL_VERSION: &str = "2025-06-18";

pub fn serve() -> Result<()> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<Value>(&line) {
            Ok(request) => handle(&request),
            Err(e) => Some(error(Value::Null, -32700, &format!("parse error: {e}"))),
        };
        if let Some(response) = response {
            writeln!(stdout, "{response}")?;
            stdout.flush()?;
        }
    }
    Ok(())
}

/// The response to one JSON-RPC message; `None` for notifications.
pub fn handle(request: &Value) -> Option<Value> {
    let id = request.get("id").cloned();
    let method = request["method"].as_str().unwrap_or_default();
    let id = id?;
    let result = match method {
        "initialize" => json!({
            "protocolVersion": request["params"]["protocolVersion"].as_str().unwrap_or(PROTOCOL_VERSION),
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "carcinize", "version": env!("CARGO_PKG_VERSION") },
            "instructions": "Incremental TS/JS to Rust migration. Loop per unit: analyze -> bridge -> \
                record -> (port src/port.rs) -> build -> diff -> shadow -> gate. Never mark a port done \
                before gate passes.",
        }),
        "ping" => json!({}),
        "tools/list" => json!({ "tools": tools() }),
        "tools/call" => {
            let name = request["params"]["name"].as_str().unwrap_or_default();
            let args = &request["params"]["arguments"];
            match call(name, args) {
                Ok(value) => json!({
                    "content": [{ "type": "text", "text": serde_json::to_string_pretty(&value).unwrap_or_default() }],
                    "structuredContent": value,
                    "isError": false,
                }),
                Err(e) => json!({
                    "content": [{ "type": "text", "text": format!("{e:#}") }],
                    "isError": true,
                }),
            }
        }
        _ => return Some(error(id, -32601, &format!("unknown method `{method}`"))),
    };
    Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn tools() -> Value {
    let crate_dir = json!({ "type": "string", "description": "Folder of the generated crate (holds carcinize.json)." });
    json!([
        {
            "name": "analyze",
            "description": "Map a TS/JS repository: units, port order (waves), risks with file:line, bridge per unit, npm-to-crate fit, and `start_here`: the best first ports.",
            "inputSchema": { "type": "object", "properties": { "root": { "type": "string", "description": "Repository root." } }, "required": ["root"] },
        },
        {
            "name": "bridge",
            "description": "Scaffold a Rust crate for one unit: napi-rs or wasm-bindgen bindings in src/lib.rs, stubs to port in src/port.rs, UTF-16 helpers when the unit uses string offsets. Never overwrites an existing folder.",
            "inputSchema": { "type": "object", "properties": {
                "root": { "type": "string" },
                "unit": { "type": "string", "description": "Unit id from analyze." },
                "kind": { "type": "string", "enum": ["napi", "wasm"], "description": "Defaults to the plan's bridge." },
                "out": { "type": "string", "description": "Crate folder; defaults to <root>/rust/<name>." },
                "force": { "type": "boolean", "description": "Bridge a unit the plan marks blocked or keep." },
                "keep": { "type": "array", "items": { "type": "string" }, "description": "Exports to keep in TS: `name` or `name: reason`." },
            }, "required": ["root", "unit"] },
        },
        {
            "name": "refresh",
            "description": "Regenerate a crate's bindings (lib.rs, index.cjs, js.rs) from the signatures in src/port.rs, after changing them. Never touches port.rs.",
            "inputSchema": { "type": "object", "properties": { "crate": crate_dir }, "required": ["crate"] },
        },
        {
            "name": "record",
            "description": "Run the original TS functions on generated edge-case inputs (non-ASCII, emoji, -0, NaN, ...) plus optional extra inputs, and save the results as cases.jsonl: the behavior the port must match.",
            "inputSchema": { "type": "object", "properties": {
                "crate": crate_dir,
                "count": { "type": "integer", "description": "Generated cases per function (default 200)." },
                "inputs": { "type": "string", "description": "JSONL file of extra cases: {\"fn\": name, \"args\": [...]}." },
                "module": { "type": "string", "description": "Import the original from this JS module instead of the TS source." },
                "resolve_from": { "type": "string", "description": "Resolve npm packages from this folder's node_modules." },
            }, "required": ["crate"] },
        },
        {
            "name": "build",
            "description": "Build the crate's JS module (napi-rs .node or wasm-bindgen output).",
            "inputSchema": { "type": "object", "properties": { "crate": crate_dir }, "required": ["crate"] },
        },
        {
            "name": "diff",
            "description": "Replay cases.jsonl against the built Rust module and list mismatches.",
            "inputSchema": { "type": "object", "properties": {
                "crate": crate_dir,
                "ignore_key_order": { "type": "boolean" },
            }, "required": ["crate"] },
        },
        {
            "name": "bench",
            "description": "Time the recorded cases on the original and on the built Rust module, bridge included; speedup below 1 means the port is slower as its callers call it.",
            "inputSchema": { "type": "object", "properties": {
                "crate": crate_dir,
                "resolve_from": { "type": "string" },
            }, "required": ["crate"] },
        },
        {
            "name": "shadow",
            "description": "Generate a drop-in TS module that runs the original and the Rust port side by side and reports mismatches; CARCINIZE_MODE=old|shadow|new switches at runtime.",
            "inputSchema": { "type": "object", "properties": { "crate": crate_dir, "out": { "type": "string" } }, "required": ["crate"] },
        },
        {
            "name": "gate",
            "description": "Definition of done: fmt, clippy -D warnings, cargo test, unsafe with SAFETY comments only, no unimplemented functions, build, API present, recorded behavior matches.",
            "inputSchema": { "type": "object", "properties": { "crate": crate_dir }, "required": ["crate"] },
        },
    ])
}

fn call(name: &str, args: &Value) -> Result<Value> {
    let path = |key: &str| -> Result<PathBuf> {
        args[key]
            .as_str()
            .map(PathBuf::from)
            .with_context(|| format!("missing `{key}`"))
    };
    let opt_path = |key: &str| args[key].as_str().map(PathBuf::from);
    let no_node_args: [String; 0] = [];
    Ok(match name {
        "analyze" => serde_json::to_value(analyze::analyze(&path("root")?)?)?,
        "bridge" => {
            let kind = match args["kind"].as_str() {
                Some("napi") => Some(bridge::Kind::Napi),
                Some("wasm") => Some(bridge::Kind::Wasm),
                _ => None,
            };
            let root = path("root")?;
            serde_json::to_value(bridge::bridge(&bridge::Options {
                root: &root,
                unit: args["unit"].as_str().context("missing `unit`")?,
                kind,
                out: opt_path("out"),
                name: None,
                force: args["force"].as_bool().unwrap_or(false),
                keep: args["keep"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default(),
                lang: None,
            })?)?
        }
        "record" => {
            let crate_dir = path("crate")?;
            let inputs = opt_path("inputs");
            let module = opt_path("module");
            let resolve_from = opt_path("resolve_from");
            serde_json::to_value(harness::record(&harness::RecordOptions {
                crate_dir: &crate_dir,
                count: args["count"]
                    .as_u64()
                    .map_or(200, |n| usize::try_from(n).unwrap_or(200)),
                inputs: inputs.as_deref(),
                module: module.as_deref(),
                resolve_from: resolve_from.as_deref(),
                node_args: &no_node_args,
            })?)?
        }
        "refresh" => serde_json::to_value(bridge::refresh(&path("crate")?)?)?,
        "build" => json!({ "artifact": harness::build(&path("crate")?)? }),
        "diff" => {
            let crate_dir = path("crate")?;
            serde_json::to_value(harness::diff(&harness::DiffOptions {
                crate_dir: &crate_dir,
                module: None,
                ignore_key_order: args["ignore_key_order"].as_bool().unwrap_or(false),
                node_args: &no_node_args,
            })?)?
        }
        "bench" => {
            let crate_dir = path("crate")?;
            let resolve_from = opt_path("resolve_from");
            serde_json::to_value(harness::bench(&harness::BenchOptions {
                crate_dir: &crate_dir,
                min_ms: 200,
                resolve_from: resolve_from.as_deref(),
                node_args: &no_node_args,
            })?)?
        }
        "shadow" => {
            let out = opt_path("out");
            serde_json::to_value(shadow::shadow(&path("crate")?, out.as_deref())?)?
        }
        "gate" => serde_json::to_value(gate::gate(Path::new(&path("crate")?), &no_node_args)?)?,
        _ => anyhow::bail!("unknown tool `{name}`"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speaks_mcp() {
        let init = handle(&json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-06-18" } })).unwrap();
        assert_eq!(init["result"]["serverInfo"]["name"], "carcinize");
        assert!(
            handle(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).is_none()
        );
        let list = handle(&json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" })).unwrap();
        let names: Vec<&str> = list["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            [
                "analyze", "bridge", "refresh", "record", "build", "diff", "bench", "shadow",
                "gate"
            ]
        );
        let bad = handle(&json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": "gate", "arguments": {} } })).unwrap();
        assert_eq!(bad["result"]["isError"], true);
        let unknown = handle(&json!({ "jsonrpc": "2.0", "id": 4, "method": "nope" })).unwrap();
        assert_eq!(unknown["error"]["code"], -32601);
    }
}
