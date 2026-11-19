import fs from "node:fs"; import path from "node:path";
const [prettierRoot, realRoot, out] = process.argv.slice(2);
const lines = []; const used = new Set();
const name = (s) => { let n = s.replace(/[^A-Za-z0-9]+/g, "-").replace(/^-|-$/g, "").toLowerCase(); if (n.length > 96) n = n.slice(0, 40) + "-" + n.slice(-55); let k = n, i = 2; while (used.has(k)) k = `${n}-${i++}`; used.add(k); return k; };
const push = (n, file, text, extra = []) => lines.push(JSON.stringify({ name: name(n), args: [...extra, "--stdin-filepath", file], stdin: text }));
const walk = (d, out, skip = []) => { for (const e of fs.readdirSync(d, { withFileTypes: true }).sort((a, b) => (a.name < b.name ? -1 : 1))) { const p = path.join(d, e.name);
  if (e.isDirectory()) { if (!["__snapshots__", "node_modules", ".git", ...skip].includes(e.name)) walk(p, out, skip); }
  else if (/\.(ya?ml)$/.test(e.name)) out.push(p); } };
const fixtures = []; walk(path.join(prettierRoot, "tests/format/yaml"), fixtures);
walk(path.join(prettierRoot, "tests/format/misc"), fixtures);
const texts = fixtures.map((f) => ({ rel: path.relative(prettierRoot, f).replace(/^tests\/format\//, ""), text: fs.readFileSync(f, "utf8") }));
for (const f of texts) push(`yaml-${f.rel}`, f.rel, f.text);
for (const f of texts) push(`as.yaml-${f.rel}`, f.rel.replace(/\.[^.\/]+$/, "") + ".yaml", f.text);
const variants = [["--prose-wrap", "always"], ["--prose-wrap", "never"], ["--tab-width", "4"], ["--no-bracket-spacing"], ["--single-quote"], ["--print-width", "40"]];
for (const v of variants) for (const f of texts) push(`${v.join("-")}-${f.rel}`, f.rel, f.text, v);
for (const f of texts) push(`crlf-${f.rel}`, f.rel, f.text.replace(/\n/g, "\r\n"));
const real = [];
for (const d of [".github", ""]) { const p = path.join(prettierRoot, d); if (!fs.existsSync(p)) continue; if (d) walk(p, real); else for (const e of fs.readdirSync(p)) { const f = path.join(p, e); if (fs.statSync(f).isFile() && /\.ya?ml$/.test(e)) real.push(f); } }
if (fs.existsSync(realRoot)) walk(realRoot, real);
for (const f of real) { const rel = f.startsWith(path.resolve(realRoot)) || f.startsWith(realRoot) ? "real/" + path.relative(realRoot, f) : "prettier-repo/" + path.relative(prettierRoot, f); push(rel, rel, fs.readFileSync(f, "utf8")); }
const snippets = {
  empty: "", spaces: "  \n", "only-comment": "# x\n", scalar: "hello\n", "map": "a: 1\nb: two\nc:\n", nested: "a:\n  b:\n    c: [1, 2, {d: e}]\n  f: {}\n",
  seq: "- a\n- b:\n    c\n-\n- - x\n  - y\n", "flow-long": "key: [aaaaaaaaaa, bbbbbbbbbbbb, cccccccccccc, dddddddddddd, eeeeeeeeeeee, ffffffffff]\n",
  quotes: "a: 'single'\nb: \"double\"\nc: 'it''s'\nd: \"say \\\"hi\\\"\"\ne: \"tab\\there\"\nf: 'has \"dq\"'\n",
  block: "lit: |\n  line1\n   line2\nfold: >-\n  some folded\n  text here\n\n  para\nkeep: |+\n  kept\n\n",
  anchors: "base: &b\n  x: 1\nother:\n  <<: *b\n  y: 2\nlist: &l [1, 2]\nref: *l\n", tags: "a: !!str 1\nb: !custom value\nc: !!map {x: 1}\nset: !!set\n  ? a\n  ? b\n",
  docs: "a: 1\n---\nb: 2\n...\n---\nc: 3\n", directives: "%YAML 1.2\n%TAG !e! tag:example.com,2000:\n---\na: !e!foo 1\n",
  comments: "# head\na: 1 # trailing\n# between\nb:\n  # inside\n  c: 2\n\n# end\n", "explicit-key": "? a\n: 1\n? [x, y]\n: z\n? |\n  block key\n: v\n",
  "long-plain": "description: this is a fairly long plain scalar that goes well past the print width of eighty characters\n",
  "multi-line-plain": "a: first line\n  second line\n  third\n", "empty-values": "a:\nb: ~\nc: null\n- x\n",
  "dup-keys": "a: 1\na: 2\n", "dup-merge": "<<: *x\n<<: *y\n", "bad-indent": "a:\n  b: 1\n c: 2\n", "tab": "a:\n\tb: 1\n", "unclosed": "a: [1, 2\n", "unclosed-quote": "a: \"x\n",
  "alias-missing": "a: *nope\n", "crlf": "a: 1\r\nb:\r\n  - c\r\n", bom: "\ufeffa: 1\n", "no-newline": "a: 1", "prettier-ignore": "# prettier-ignore\na:   [1,2,  3]\nb:   [1,2,  3]\n",
  "seq-in-map-indent": "a:\n- 1\n- 2\nb:\n  - 3\n", "flow-comments": "a: [\n  1, # one\n  2\n]\n", "key-with-spaces": "a key with spaces: value\n\"quoted key\": v\n",
  "numbers": "a: 0o17\nb: 0x1F\nc: 1e3\nd: .inf\ne: -.Inf\nf: .NaN\ng: 1_000\n", "special-strings": "a: '@at'\nb: \"`tick`\"\nc: yes\nd: on\n",
  "github-workflow": "name: CI\non:\n  push:\n    branches: [main]\njobs:\n  build:\n    runs-on: ${{ matrix.os }}\n    strategy:\n      matrix:\n        os: [ubuntu-latest, macos-latest]\n    steps:\n      - uses: actions/checkout@v4\n      - run: |\n          npm ci\n          npm test\n",
  "docker-compose": "version: '3'\nservices:\n  web:\n    image: \"nginx:latest\"\n    ports:\n      - \"80:80\"\n    environment:\n      - KEY=value\n",
};
for (const [k, v] of Object.entries(snippets)) for (const ext of [".yml", ".yaml"]) push(`snippet-${k}${ext}`, `snippet/${k}${ext}`, v);
for (const [k, v] of Object.entries(snippets)) push(`snippet-${k}-w40`, `snippet/${k}.yml`, v, ["--print-width", "40"]);
for (const n of [".clang-format", "CITATION.cff", ".prettierrc", "yarn.lock", ".gemrc"]) push(`filename-${n}`, `dir/${n}`, snippets.map);
fs.writeFileSync(out, lines.join("\n") + "\n"); console.log({ fixtures: texts.length, real: real.length, total: lines.length });
