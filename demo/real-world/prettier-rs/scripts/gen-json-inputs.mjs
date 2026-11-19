import fs from "node:fs"; import path from "node:path";
const [prettierRoot, realRoot, out] = process.argv.slice(2);
const lines = []; const used = new Set();
const name = (s) => { let n = s.replace(/[^A-Za-z0-9]+/g, "-").replace(/^-|-$/g, "").toLowerCase(); if (n.length > 96) n = n.slice(0, 40) + "-" + n.slice(-55); let k = n, i = 2; while (used.has(k)) k = `${n}-${i++}`; used.add(k); return k; };
const push = (n, file, text, extra = []) => lines.push(JSON.stringify({ name: name(n), args: [...extra, "--stdin-filepath", file], stdin: text }));
const walk = (d, out) => { for (const e of fs.readdirSync(d, { withFileTypes: true }).sort((a, b) => (a.name < b.name ? -1 : 1))) { const p = path.join(d, e.name);
  if (e.isDirectory()) { if (!["node_modules", ".git", "__snapshots__", "dist"].includes(e.name)) walk(p, out); }
  else if (/\.(json|json5|jsonc)$/.test(e.name) || [".babelrc", ".swcrc"].includes(e.name)) out.push(p); } };
const fixtures = []; walk(path.join(prettierRoot, "tests/format/json"), fixtures);
for (const d of ["insert-pragma", "require-pragma", "check-ignore-pragma"]) { const p = path.join(prettierRoot, "tests/format/misc", d); if (fs.existsSync(p)) walk(p, fixtures); }
const texts = fixtures.map((f) => ({ rel: path.relative(prettierRoot, f).replace(/^tests\/format\//, ""), text: fs.readFileSync(f, "utf8") }));
for (const f of texts) push(`json-${f.rel}`, f.rel, f.text);
for (const f of texts) {
  const base = f.rel.replace(/\.[^.\/]+$/, "");
  for (const ext of [".json", ".jsonc", ".json5"]) if (!f.rel.endsWith(ext)) push(`as${ext}-${f.rel}`, base + ext, f.text);
  push(`as-package-${f.rel}`, path.dirname(f.rel) + "/package.json", f.text);
}
const variants = [["--tab-width", "4"], ["--use-tabs"], ["--print-width", "40"], ["--single-quote"], ["--quote-props", "preserve"], ["--quote-props", "consistent"], ["--trailing-comma", "all"], ["--trailing-comma", "none"], ["--no-bracket-spacing"], ["--object-wrap", "collapse"]];
for (const v of variants) for (const f of texts) for (const ext of [".json", ".json5"]) push(`${v.join("-")}${ext}-${f.rel}`, f.rel.replace(/\.[^.\/]+$/, "") + ext, f.text, v);
const real = []; walk(realRoot, real);
for (const d of ["", ".vscode", "website", "scripts"]) { const p = path.join(prettierRoot, d); if (!fs.existsSync(p)) continue; for (const e of fs.readdirSync(p)) { const f = path.join(p, e); if (fs.statSync(f).isFile() && /\.(json|jsonc|json5)$/.test(e)) real.push(f); } }
for (const f of real) { const rel = f.startsWith(realRoot) ? "real/" + path.relative(realRoot, f) : "prettier-repo/" + path.relative(prettierRoot, f); push(rel, rel, fs.readFileSync(f, "utf8")); }
const snippets = {
  empty: "", spaces: "  \n", "only-comment": "// nothing\n", "block-comment-only": "/* x */\n",
  numbers: '[1, 1.0, 1.50, 1e10, 1E-7, 0.1, .5, 5., 0x1F, -1, +1, -0, 1_000]\n',
  specials: '[Infinity, -Infinity, NaN, -NaN, +Infinity, undefined, null, true, false]\n',
  strings: `["a", 'b', "c'd", 'e"f', "\\u00e9", "line\\nbreak", "tab\\t", 'it\\'s', "\\/"]\n`,
  "unquoted-keys": "{a: 1, b_c: 2, $d: 3, 1: 4, 1.5: 5, 'e': 6, \"f\": 7}\n",
  "trailing-commas": '{"a": [1, 2, 3,], "b": {"c": 1,},}\n',
  comments: '{\n  // line\n  "a": 1, /* inline */ "b": 2,\n  /* block\n     comment */\n  "c": [1, // after one\n    2]\n}\n',
  template: '{"a": `hello`}\n', "template-expr": '{"a": `${b}`}\n',
  "long-array": JSON.stringify(Array.from({ length: 60 }, (_, i) => i * 7)) + "\n",
  "long-strings": JSON.stringify(Array.from({ length: 8 }, (_, i) => "item number " + i + " with some text")) + "\n",
  nested: JSON.stringify({ a: { b: { c: { d: [1, { e: [2, 3, { f: "g" }] }] } } }, h: [], i: {} }) + "\n",
  "pretty-nested": JSON.stringify({ name: "x", version: "1.0.0", scripts: { build: "tsc", test: "vitest run --coverage" }, deps: ["a", "b"] }, null, 4) + "\n",
  "empty-containers": '{"a": {}, "b": [], "c": [{}], "d": [[]]}\n', "deep-arrays": "[[[[1,2],[3,4]],[[5,6],[7,8]]]]\n",
  "object-first-line": '{"a": 1,\n"b": 2}\n', "object-same-line": '{ "a": 1, "b": {"c": 2,\n"d": 3}}\n',
  "unicode-keys": '{"é": 1, "日本": 2, "a b": 3}\n', "crlf": '{\r\n  "a": 1\r\n}\r\n', bom: '﻿{"a": 1}\n',
  "regex": '{"a": /x/}\n', bigint: '{"a": 1n}\n', "paren": '{"a": (1)}\n', "computed": '{["a"]: 1}\n', shorthand: "{a}\n",
  "neg-string": '{"a": -"x"}\n', identifier: '{"a": foo}\n', "string-root": '"just a string"\n', "number-root": "42\n",
  "array-of-objects": JSON.stringify([{ id: 1, name: "a" }, { id: 2, name: "b" }]) + "\n",
};
for (const [k, v] of Object.entries(snippets)) for (const ext of [".json", ".jsonc", ".json5"]) push(`snippet-${k}${ext}`, `snippet/${k}${ext}`, v);
for (const [k, v] of Object.entries(snippets)) push(`snippet-${k}-package`, `snippet-${k}/package.json`, v);
fs.writeFileSync(out, lines.join("\n") + "\n"); console.log({ fixtures: texts.length, real: real.length, total: lines.length });
