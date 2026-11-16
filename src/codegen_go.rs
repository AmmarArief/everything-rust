//! Go: the cgo wrapper package (the original API over the Rust library), and the driver that
//! calls a package's functions by name for record, diff and bench.

use std::fmt::Write as _;
use std::path::Path;

use crate::bridge::Manifest;
use crate::ts::exports::ExportedFn;

/// The result types of a Go function, without the trailing `error`, and whether it has one.
pub fn results(f: &ExportedFn) -> (Vec<String>, bool) {
    let text = f.native_return.as_deref().unwrap_or("()");
    let inner = text.trim().trim_start_matches('(').trim_end_matches(')');
    let mut parts = split_top(inner);
    let fallible = parts.last().is_some_and(|p| p == "error");
    if fallible {
        parts.pop();
    }
    (parts, fallible)
}

fn split_top(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut current = String::new();
    for c in text.chars() {
        match c {
            '[' | '(' | '{' => depth += 1,
            ']' | ')' | '}' => depth -= 1,
            ',' if depth == 0 => {
                out.push(current.trim().to_string());
                current.clear();
                continue;
            }
            _ => {}
        }
        current.push(c);
    }
    if !current.trim().is_empty() {
        out.push(current.trim().to_string());
    }
    out
}

/// Exported identifiers in a Go type: the package's own named types.
fn local_types(t: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut word = String::new();
    let mut prev = ' ';
    for c in t.chars().chain(std::iter::once(' ')) {
        if c.is_alphanumeric() || c == '_' {
            word.push(c);
            continue;
        }
        if !word.is_empty() {
            if word.starts_with(char::is_uppercase) && prev != '.' {
                out.push(std::mem::take(&mut word));
            }
            word.clear();
        }
        prev = c;
    }
    out
}

/// `t` with the package's named types qualified as `pkg.Name`.
fn qualify(t: &str) -> String {
    let mut out = String::new();
    let mut word = String::new();
    let mut prev = ' ';
    for c in t.chars().chain(std::iter::once('\0')) {
        if c.is_alphanumeric() || c == '_' {
            word.push(c);
            continue;
        }
        if !word.is_empty() {
            if word.starts_with(char::is_uppercase) && prev != '.' {
                out.push_str("pkg.");
            }
            out.push_str(&word);
            word.clear();
        }
        if c != '\0' {
            out.push(c);
        }
        prev = c;
    }
    out
}

/// Every named type the exported signatures use.
pub fn used_types(m: &Manifest) -> Vec<String> {
    let mut names: Vec<String> = m
        .exports
        .iter()
        .flat_map(|f| {
            f.params
                .iter()
                .filter_map(|p| p.native_type.clone())
                .chain(results(f).0)
                .collect::<Vec<_>>()
        })
        .flat_map(|t| local_types(&t))
        .collect();
    names.sort();
    names.dedup();
    names
}

/// The Go sources of the unit (not tests).
pub fn unit_sources(m: &Manifest) -> Vec<String> {
    let dir = m.root.join(m.unit.trim_end_matches('/'));
    let dir = if dir.is_file() {
        dir.parent().map(Path::to_path_buf).unwrap_or(dir)
    } else {
        dir
    };
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension().is_some_and(|e| e == "go") && !p.to_string_lossy().ends_with("_test.go")
        })
        .collect();
    files.sort();
    files
        .iter()
        .filter_map(|p| std::fs::read_to_string(p).ok())
        .collect()
}

pub fn package_name(m: &Manifest) -> String {
    unit_sources(m)
        .iter()
        .find_map(|s| crate::lang::go::package_name(s))
        .unwrap_or_else(|| "port".into())
}

/// JSON helpers shared by the wrapper and the driver.
const HELPERS_GO: &str = r#"
// carcinizeDecode decodes JSON into target; floats may be {"$num": "NaN" | "Infinity" | ...}.
func carcinizeDecode(raw json.RawMessage, target any) error {
	if f, ok := target.(*float64); ok && len(raw) > 0 && raw[0] == '{' {
		var marker map[string]string
		if err := json.Unmarshal(raw, &marker); err != nil {
			return err
		}
		switch marker["$num"] {
		case "NaN":
			*f = math.NaN()
		case "Infinity":
			*f = math.Inf(1)
		case "-Infinity":
			*f = math.Inf(-1)
		case "-0":
			*f = math.Copysign(0, -1)
		default:
			return errors.New("not a number")
		}
		return nil
	}
	if len(raw) == 0 {
		raw = json.RawMessage("null")
	}
	return json.Unmarshal(raw, target)
}

// carcinizeEncode marks the floats JSON cannot carry.
func carcinizeEncode(v any) any {
	var f float64
	switch x := v.(type) {
	case float64:
		f = x
	case float32:
		f = float64(x)
	default:
		return v
	}
	switch {
	case math.IsNaN(f):
		return map[string]string{"$num": "NaN"}
	case math.IsInf(f, 1):
		return map[string]string{"$num": "Infinity"}
	case math.IsInf(f, -1):
		return map[string]string{"$num": "-Infinity"}
	case f == 0 && math.Signbit(f):
		return map[string]string{"$num": "-0"}
	}
	return v
}
"#;

/// The wrapper package: the original package's API, each function calling the Rust library.
pub fn wrapper(m: &Manifest) -> String {
    let lib = m.lib_name();
    let sources = unit_sources(m);
    let types = crate::lang::go::type_decls(&sources, &used_types(m));
    let mut out = format!(
        "// Package {pkg} is the Rust port of `{unit}`, generated by carcinize: the same API, each\n\
         // function calling the Rust library. Regenerate with `carcinize refresh`.\n\
         package {pkg}\n\n\
         /*\n#cgo LDFLAGS: -L${{SRCDIR}}/../target/release -l{lib} -Wl,-rpath,${{SRCDIR}}/../target/release\n\
         #include <stdlib.h>\n\
         char *carcinize_call(const char *name, const char *args);\n\
         void carcinize_free(char *ptr);\n*/\nimport \"C\"\n\n\
         import (\n\t\"encoding/json\"\n\t\"errors\"\n\t\"math\"\n\t\"unsafe\"\n)\n",
        pkg = package_name(m),
        unit = m.unit,
    );
    for t in types {
        let _ = write!(out, "\n{t}\n");
    }
    out.push_str(HELPERS_GO);
    out.push_str(
        r#"
type carcinizeAnswer struct {
	Ok    json.RawMessage `json:"ok"`
	Error *string         `json:"error"`
	Panic *string         `json:"panic"`
}

func carcinizeCall(name string, args ...any) (json.RawMessage, error) {
	for i, a := range args {
		args[i] = carcinizeEncode(a)
	}
	encoded, err := json.Marshal(args)
	if err != nil {
		panic(err)
	}
	cName := C.CString(name)
	defer C.free(unsafe.Pointer(cName))
	cArgs := C.CString(string(encoded))
	defer C.free(unsafe.Pointer(cArgs))
	answer := C.carcinize_call(cName, cArgs)
	defer C.carcinize_free(answer)
	var a carcinizeAnswer
	if err := json.Unmarshal([]byte(C.GoString(answer)), &a); err != nil {
		panic(err)
	}
	if a.Panic != nil {
		panic(*a.Panic)
	}
	if a.Error != nil {
		return nil, errors.New(*a.Error)
	}
	return a.Ok, nil
}

func carcinizeMust(err error) {
	if err != nil {
		panic(err)
	}
}
"#,
    );
    for f in &m.exports {
        let (results, fallible) = results(f);
        let params: Vec<String> = f
            .params
            .iter()
            .map(|p| format!("{} {}", p.name, p.native_type.as_deref().unwrap_or("any")))
            .collect();
        let mut all_results = results.clone();
        if fallible {
            all_results.push("error".into());
        }
        let ret = match all_results.len() {
            0 => String::new(),
            1 => format!(" {}", all_results[0]),
            _ => format!(" ({})", all_results.join(", ")),
        };
        let args: Vec<&str> = f.params.iter().map(|p| p.name.as_str()).collect();
        let call_args = if args.is_empty() {
            String::new()
        } else {
            format!(", {}", args.join(", "))
        };
        let _ = writeln!(
            out,
            "\n// {} is the Rust port of `{}` (`{}:{}`).\nfunc {}({}){ret} {{",
            f.name,
            f.prototype.as_deref().unwrap_or(&f.name),
            f.file,
            f.line,
            f.name,
            params.join(", ")
        );
        let names: Vec<String> = (0..results.len()).map(|i| format!("r{i}")).collect();
        let decls: String = results
            .iter()
            .enumerate()
            .map(|(i, t)| format!("\tvar r{i} {t}\n"))
            .collect();
        let raw = if results.is_empty() { "_" } else { "raw" };
        let _ = writeln!(
            out,
            "\t{raw}, err := carcinizeCall(\"{}\"{call_args})",
            f.name
        );
        if fallible {
            let _ = writeln!(out, "\tif err != nil {{\n{decls}\t\treturn {}\n\t}}", {
                let mut r = names.clone();
                r.push("err".into());
                r.join(", ")
            });
        } else {
            out.push_str("\tcarcinizeMust(err)\n");
        }
        out.push_str(&decls);
        match results.len() {
            0 => {}
            1 => out.push_str("\tcarcinizeMust(carcinizeDecode(raw, &r0))\n"),
            n => {
                let _ = writeln!(
                    out,
                    "\tvar parts []json.RawMessage\n\tcarcinizeMust(json.Unmarshal(raw, &parts))\n\
                     \tif len(parts) != {n} {{\n\t\tpanic(\"carcinize: expected {n} results\")\n\t}}"
                );
                for i in 0..n {
                    let _ = writeln!(out, "\tcarcinizeMust(carcinizeDecode(parts[{i}], &r{i}))");
                }
            }
        }
        let mut r = names;
        if fallible {
            r.push("nil".into());
        }
        if r.is_empty() {
            out.push_str("}\n");
        } else {
            let _ = writeln!(out, "\treturn {}\n}}", r.join(", "));
        }
    }
    out
}

/// A `main` package that calls `import_path`'s functions by name: record, diff and bench.
pub fn driver(m: &Manifest, import_path: &str) -> String {
    let mut cases = String::new();
    for f in &m.exports {
        let (results, fallible) = results(f);
        let _ = writeln!(cases, "\tcase \"{}\":", f.name);
        let mut args = Vec::new();
        for (i, p) in f.params.iter().enumerate() {
            let t = qualify(p.native_type.as_deref().unwrap_or("any"));
            let _ = writeln!(
                cases,
                "\t\tvar a{i} {t}\n\t\tif carcinizeDecode(argAt(args, {i}), &a{i}) != nil {{\n\t\t\treturn invalid\n\t\t}}"
            );
            args.push(format!("a{i}"));
        }
        let mut lhs: Vec<String> = (0..results.len()).map(|i| format!("r{i}")).collect();
        if fallible {
            lhs.push("err".into());
        }
        let call = format!("pkg.{}({})", f.name, args.join(", "));
        if lhs.is_empty() {
            let _ = writeln!(cases, "\t\t{call}");
        } else {
            let _ = writeln!(cases, "\t\t{} := {call}", lhs.join(", "));
        }
        if fallible {
            cases.push_str(
                "\t\tif err != nil {\n\t\t\treturn map[string]any{\"error\": err.Error()}\n\t\t}\n",
            );
        }
        let ok = match results.len() {
            0 => "nil".to_string(),
            1 => "carcinizeEncode(r0)".to_string(),
            n => format!(
                "[]any{{{}}}",
                (0..n)
                    .map(|i| format!("carcinizeEncode(r{i})"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        };
        let _ = writeln!(cases, "\t\treturn map[string]any{{\"ok\": {ok}}}");
    }
    DRIVER_GO
        .replace("IMPORT_PATH", import_path)
        .replace("// CASES\n", &cases)
        .replace("// HELPERS\n", HELPERS_GO)
}

const DRIVER_GO: &str = r#"// Generated by carcinize: calls the package's functions by name.
package main

import (
	"bufio"
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"math"
	"os"
	"time"

	pkg "IMPORT_PATH"
)

var _ = errors.New

// HELPERS

var invalid = map[string]any{"invalid": true}

func argAt(args []json.RawMessage, i int) json.RawMessage {
	if i < len(args) {
		return args[i]
	}
	return nil
}

func call(name string, args []json.RawMessage) (out map[string]any) {
	defer func() {
		if r := recover(); r != nil {
			out = map[string]any{"panic": fmt.Sprint(r)}
		}
	}()
	switch name {
	// CASES
	}
	return map[string]any{"missing": name}
}

type input struct {
	Fn   string            `json:"fn"`
	Args []json.RawMessage `json:"args"`
}

// canon re-encodes a JSON value with exact numbers, so equal values compare equal as text.
func canon(v any) string {
	text, err := json.Marshal(v)
	if err != nil {
		return "unencodable: " + err.Error()
	}
	d := json.NewDecoder(bytes.NewReader(text))
	d.UseNumber()
	var x any
	if err := d.Decode(&x); err != nil {
		return string(text)
	}
	out, _ := json.Marshal(x)
	return string(out)
}

func record(inputs, out string) {
	data, err := os.ReadFile(inputs)
	if err != nil {
		panic(err)
	}
	var cases []input
	if err := json.Unmarshal(data, &cases); err != nil {
		panic(err)
	}
	f, err := os.Create(out)
	if err != nil {
		panic(err)
	}
	defer f.Close()
	w := bufio.NewWriter(f)
	defer w.Flush()
	for _, c := range cases {
		result := call(c.Fn, c.Args)
		if result["invalid"] != nil {
			continue
		}
		line := map[string]any{"fn": c.Fn, "args": c.Args}
		for k, v := range result {
			line[k] = v
		}
		text, err := json.Marshal(line)
		if err != nil {
			continue
		}
		w.Write(text)
		w.WriteByte('\n')
	}
}

func readCases(path string) []map[string]json.RawMessage {
	data, err := os.ReadFile(path)
	if err != nil {
		panic(err)
	}
	var out []map[string]json.RawMessage
	for _, line := range bytes.Split(data, []byte("\n")) {
		if len(bytes.TrimSpace(line)) == 0 {
			continue
		}
		var c map[string]json.RawMessage
		if err := json.Unmarshal(line, &c); err != nil {
			panic(err)
		}
		out = append(out, c)
	}
	return out
}

func argsOf(c map[string]json.RawMessage) (string, []json.RawMessage) {
	var name string
	var args []json.RawMessage
	json.Unmarshal(c["fn"], &name)
	json.Unmarshal(c["args"], &args)
	return name, args
}

func diff(cases, out string) {
	summary := map[string]any{"total": 0, "passed": 0, "failed": 0}
	missing := []string{}
	mismatches := []any{}
	total, passed, failed := 0, 0, 0
	for _, c := range readCases(cases) {
		total++
		name, args := argsOf(c)
		got := call(name, args)
		if got["missing"] != nil {
			failed++
			missing = append(missing, name)
			continue
		}
		expected := map[string]any{}
		for _, k := range []string{"ok", "error", "panic"} {
			if raw, ok := c[k]; ok {
				expected[k] = raw
			}
		}
		same := false
		if _, ok := expected["panic"]; ok {
			// Panic messages differ between Go and Rust; that it panicked is the behavior.
			_, same = got["panic"]
		} else {
			same = canon(expected) == canon(got)
		}
		if same {
			passed++
		} else {
			failed++
			if len(mismatches) < 20 {
				mismatches = append(mismatches, map[string]any{"fn": name, "args": args, "expected": expected, "actual": got})
			}
		}
	}
	summary["total"], summary["passed"], summary["failed"] = total, passed, failed
	summary["missing"], summary["mismatches"] = missing, mismatches
	text, _ := json.Marshal(summary)
	os.WriteFile(out, text, 0o644)
}

func bench(cases, out string, minMs int) {
	groups := map[string][][]json.RawMessage{}
	order := []string{}
	for _, c := range readCases(cases) {
		name, args := argsOf(c)
		if _, ok := groups[name]; !ok {
			order = append(order, name)
		}
		groups[name] = append(groups[name], args)
	}
	result := map[string]any{}
	for _, name := range order {
		calls := groups[name]
		once := func() {
			for _, args := range calls {
				call(name, args)
			}
		}
		once()
		passes, start := 0, time.Now()
		for {
			once()
			passes++
			if elapsed := time.Since(start); elapsed.Milliseconds() >= int64(minMs) {
				result[name] = map[string]any{"cases": len(calls), "ns": float64(elapsed.Nanoseconds()) / float64(passes*len(calls))}
				break
			}
		}
	}
	text, _ := json.Marshal(result)
	os.WriteFile(out, text, 0o644)
}

func main() {
	switch os.Args[1] {
	case "record":
		record(os.Args[2], os.Args[3])
	case "diff":
		diff(os.Args[2], os.Args[3])
	case "bench":
		var ms int
		fmt.Sscan(os.Args[4], &ms)
		bench(os.Args[2], os.Args[3], ms)
	}
}
"#;

/// A shadow package with the original's API: each call runs the original and the Rust port
/// (through the wrapper), returns the original's results and logs mismatches.
/// `CARCINIZE_MODE=old` runs the original only, `new` the port only.
pub fn shadow(m: &Manifest, original: &str) -> String {
    let wrapper = crate::harness_go::wrapper_module(m);
    let mut out = format!(
        "// Package {pkg} runs the Go original and the Rust port of `{unit}` side by side. Generated\n\
         // by carcinize: import it in place of {original}. CARCINIZE_MODE=old|shadow|new.\n\
         package {pkg}\n\n\
         import (\n\t\"encoding/json\"\n\t\"log\"\n\t\"os\"\n\t\"reflect\"\n\t\"sync/atomic\"\n\n\
         \torig \"{original}\"\n\tport \"{wrapper}\"\n)\n\n\
         var mode = func() string {{\n\tif m := os.Getenv(\"CARCINIZE_MODE\"); m != \"\" {{\n\t\treturn m\n\t}}\n\treturn \"shadow\"\n}}()\n\n\
         // Calls and Mismatches count shadowed calls.\nvar Calls, Mismatches atomic.Int64\n\n\
         // convert moves a value between the original's and the wrapper's identical types.\n\
         func convert[T any](v any) T {{\n\tvar out T\n\tif text, err := json.Marshal(v); err == nil {{\n\t\t_ = json.Unmarshal(text, &out)\n\t}}\n\treturn out\n}}\n\n\
         func compare(name string, expected, actual []any) {{\n\tCalls.Add(1)\n\tif !reflect.DeepEqual(expected, actual) {{\n\
         \t\tif Mismatches.Add(1) <= 10 {{\n\t\t\tlog.Printf(\"[carcinize] mismatch in %s: expected %v, got %v\", name, expected, actual)\n\t\t}}\n\t}}\n}}\n",
        pkg = package_name(m),
        unit = m.unit,
    );
    for t in used_types(m) {
        let _ = write!(
            out,
            "\n// {t} is the original's type.\ntype {t} = orig.{t}\n"
        );
    }
    for f in &m.exports {
        let (results, fallible) = results(f);
        let params: Vec<String> = f
            .params
            .iter()
            .map(|p| format!("{} {}", p.name, p.native_type.as_deref().unwrap_or("any")))
            .collect();
        let mut all: Vec<String> = results.clone();
        if fallible {
            all.push("error".into());
        }
        let ret = match all.len() {
            0 => String::new(),
            1 => format!(" {}", all[0]),
            _ => format!(" ({})", all.join(", ")),
        };
        let names: Vec<String> = (0..all.len()).map(|i| format!("o{i}")).collect();
        let rust_names: Vec<String> = (0..all.len()).map(|i| format!("n{i}")).collect();
        let args: Vec<&str> = f.params.iter().map(|p| p.name.as_str()).collect();
        let port_args: Vec<String> = f
            .params
            .iter()
            .map(|p| {
                let t = p.native_type.as_deref().unwrap_or("any");
                if local_types(t).is_empty() {
                    p.name.clone()
                } else {
                    format!(
                        "convert[{}]({})",
                        qualify(t).replace("pkg.", "port."),
                        p.name
                    )
                }
            })
            .collect();
        let lhs = |v: &[String]| {
            if v.is_empty() {
                String::new()
            } else {
                format!("{} := ", v.join(", "))
            }
        };
        let back: Vec<String> = all
            .iter()
            .zip(&rust_names)
            .map(|(t, n)| {
                if local_types(t).is_empty() {
                    n.clone()
                } else {
                    format!("convert[{t}]({n})")
                }
            })
            .collect();
        let errors_text = |v: &[String]| -> Vec<String> {
            v.iter()
                .zip(&all)
                .map(|(n, t)| {
                    if t == "error" {
                        format!("errText({n})")
                    } else {
                        n.clone()
                    }
                })
                .collect()
        };
        let _ = writeln!(
            out,
            "\n// {} shadows the original `{}`.\nfunc {}({}){ret} {{",
            f.name,
            f.name,
            f.name,
            params.join(", ")
        );
        let ret_new = if all.is_empty() {
            String::new()
        } else {
            format!("return {}", back.join(", "))
        };
        let _ = writeln!(
            out,
            "\tif mode == \"new\" {{\n\t\t{}port.{}({})\n\t\t{ret_new}\n\t}}",
            lhs(&rust_names),
            f.name,
            port_args.join(", ")
        );
        let _ = writeln!(out, "\t{}orig.{}({})", lhs(&names), f.name, args.join(", "));
        let ret_old = if all.is_empty() {
            "return".to_string()
        } else {
            format!("return {}", names.join(", "))
        };
        let _ = writeln!(
            out,
            "\tif mode != \"old\" {{\n\t\tfunc() {{\n\t\t\tdefer func() {{\n\t\t\t\tif r := recover(); r != nil {{\n\
             \t\t\t\t\tcompare(\"{name}\", []any{{\"no panic\"}}, []any{{r}})\n\t\t\t\t}}\n\t\t\t}}()\n\
             \t\t\t{}port.{name}({})\n\t\t\tcompare(\"{name}\", []any{{{}}}, []any{{{}}})\n\t\t}}()\n\t}}\n\t{ret_old}\n}}",
            lhs(&rust_names),
            port_args.join(", "),
            errors_text(&names).join(", "),
            errors_text(&back).join(", "),
            name = f.name,
        );
    }
    out.push_str("\nfunc errText(err error) any {\n\tif err == nil {\n\t\treturn nil\n\t}\n\treturn err.Error()\n}\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qualifies_local_types() {
        assert_eq!(qualify("[]Point"), "[]pkg.Point");
        assert_eq!(qualify("map[string]Point"), "map[string]pkg.Point");
        assert_eq!(qualify("time.Duration"), "time.Duration");
        assert_eq!(local_types("map[string][]Point"), ["Point"]);
    }
}
