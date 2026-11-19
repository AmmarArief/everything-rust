import fs from "node:fs"; import path from "node:path";
const [prettierRoot, out] = process.argv.slice(2);
const lines = []; const used = new Set();
const name = (s) => { let n = s.replace(/[^A-Za-z0-9]+/g, "-").replace(/^-|-$/g, "").toLowerCase(); if (n.length > 96) n = n.slice(0, 40) + "-" + n.slice(-55); let k = n, i = 2; while (used.has(k)) k = `${n}-${i++}`; used.add(k); return k; };
const push = (n, file, text, extra = []) => lines.push(JSON.stringify({ name: name(n), args: [...extra, "--stdin-filepath", file], stdin: text }));
const walk = (d, out) => { for (const e of fs.readdirSync(d, { withFileTypes: true }).sort((a, b) => (a.name < b.name ? -1 : 1))) { const p = path.join(d, e.name);
  if (e.isDirectory()) { if (!["__snapshots__"].includes(e.name)) walk(p, out); }
  else if (/\.(graphql|gql|graphqls)$/.test(e.name)) out.push(p); } };
const fixtures = []; walk(path.join(prettierRoot, "tests/format/graphql"), fixtures);
walk(path.join(prettierRoot, "tests/format/misc"), fixtures);
const texts = fixtures.map((f) => ({ rel: path.relative(prettierRoot, f).replace(/^tests\/format\//, ""), text: fs.readFileSync(f, "utf8") }));
for (const f of texts) push(`graphql-${f.rel}`, f.rel, f.text);
for (const f of texts) for (const ext of [".gql", ".graphqls"]) push(`as${ext}-${f.rel}`, f.rel.replace(/\.[^.\/]+$/, "") + ext, f.text);
const variants = [["--tab-width", "4"], ["--use-tabs"], ["--print-width", "40"], ["--print-width", "120"], ["--no-bracket-spacing"], ["--trailing-comma", "none"], ["--single-quote"], ["--end-of-line", "crlf"]];
for (const v of variants) for (const f of texts) push(`${v.join("-")}-${f.rel}`, f.rel, f.text, v);
for (const f of texts) push(`parser-graphql-${f.rel}`, "x/" + path.basename(f.rel) + ".txt", f.text, ["--parser", "graphql"]);
const snippets = {
  empty: "", spaces: "  \n", "only-comment": "# nothing\n", "two-comments": "# a\n\n# b\n",
  query: "query { a }\n", shorthand: "{ a b c }\n", "named": "query Q($a: Int = 1, $b: [String!]! @dir) @x { a(b: $a) @include(if: true) { c } }\n",
  mutation: "mutation M { like(id: 1) { count } }\n", subscription: "subscription S { s }\n",
  alias: "{ x: a, y: b(c: 1) }\n", fragments: "fragment F on T { a } { ...F ... on T { b } ... @skip(if: true) { c } }\n",
  "legacy-fragment-vars": "fragment F($a: Int = 1) on T { a }\n",
  values: '{ a(i: 1, f: 1.5e3, s: "x", b: true, n: null, e: ENUM, l: [1, 2], o: {a: 1, b: {c: [2]}}, v: $v) }\n',
  strings: '{ a(s: "q\\"uote", t: "back\\\\slash", u: "\\u00e9", n: "line\\nbreak", tab: "\\t") }\n',
  "block-strings": '"""\n  Desc\n    indented\n"""\ntype T { """single""" a: Int\n """\n multi\n\n line\n """ b: Int \n c(""" arg """ x: Int, "s" y: Int): Int }\n',
  "block-escape": '{ a(s: """has \\""" inside""") }\n', "block-empty": '{ a(s: """""", t: """   """) }\n',
  "long-args": "{ someField(argumentOne: $variableOne, argumentTwo: $variableTwo, argumentThree: $variableThree) { id } }\n",
  "long-directives": "query Q @directiveOne(arg: 1) @directiveTwo(arg: 2) @directiveThree(arg: 3) @directiveFour { a }\n",
  schema: "schema @d { query: Q mutation: M subscription: S }\nextend schema @e { query: Q }\nextend schema @f\n",
  types: "type A implements B & C @d { a(x: Int = 1 @d): [B!]! @deprecated(reason: \"no\") }\ninterface I implements J { a: Int }\nextend type A { b: Int }\nextend interface I @d\n",
  "implements-legacy": "type A implements B, C { a: Int }\ntype A implements B C { a: Int }\ntype A implements & B & C { a: Int }\n",
  "implements-comments": "type A implements B &\n# c\nC { a: Int }\n",
  union: "union U = A | B\nunion V @d = | A | B\nextend union U = C\nunion LongUnionName = FirstVeryLongTypeName | SecondVeryLongTypeName | ThirdVeryLongTypeName\nunion E\n",
  enum: "enum E { A B @deprecated C }\nextend enum E { D }\nenum F\n",
  input: "input I { a: Int = 1 @d, b: [I!] }\nextend input I { c: Int }\n",
  scalar: "scalar Date @specifiedBy(url: \"x\")\nextend scalar Date @d\n",
  directive: "directive @d(a: Int) repeatable on FIELD | QUERY\ndirective @e on | FIELD_DEFINITION | ENUM_VALUE | ARGUMENT_DEFINITION | INPUT_FIELD_DEFINITION | OBJECT\n",
  "comments-everywhere": "# top\nquery Q( # v\n  $a: Int # after a\n) {\n  # inside\n  a # trailing\n  b(\n    # arg comment\n    x: 1\n  )\n  # last\n}\n# end\n",
  "blank-lines": "{\n  a\n\n\n  b\n\n  c\n}\n\n\n\ntype T {\n  a: Int\n\n  b: Int\n}\n",
  commas: "{ a, b, c, }\ntype T { a: Int, b: Int, }\n",
  "prettier-ignore": "# prettier-ignore\nquery    Q { a   b }\n{\n  # prettier-ignore\n  a(x:   1)\n  b(y:   2)\n}\n",
  crlf: "query {\r\n  a\r\n}\r\n", bom: "﻿{ a }\n", "no-final-newline": "{ a }",
  "error-unclosed": "{ a\n", "error-bad": "query { a(: 1) }\n", "error-desc": '"d" query { a }\n', "error-const-var": "query ($a: Int = $b) { a }\n",
  "error-string": '{ a(s: "unterminated) }\n', "error-char": "{ a ~ }\n", "error-fragment-on": "fragment on on T { a }\n",
  numbers: "{ a(x: -1, y: 0, z: -0.5, w: 1e10, v: 1.5E-3) }\n", "error-number": "{ a(x: 01) }\n",
  "variable-directives": "query ($a: Int @d(x: 1) @e) { a }\n",
  "nested-lists": "{ a(x: [[1, 2], [3, [4, 5]]], y: [], z: {}) }\n",
  "long-object": "{ a(input: {firstName: \"something long\", lastName: \"another long value\", age: 42, tags: [\"one\", \"two\"]}) { id } }\n",
  "selection-long": "{ veryLongFieldNameNumberOne veryLongFieldNameNumberTwo veryLongFieldNameNumberThree }\n",
  "kitchen": fs.readFileSync(path.join(prettierRoot, "tests/format/graphql/kitchen-sink/kitchen_sink.graphql"), "utf8").replace(/\n/g, " ").replace(/#[^ ]*/g, ""),
  "comment-after-def": "type T { a: Int } # trailing\n# own line\ntype U { b: Int }\n",
  "description-arg": 'type T { f("desc" a: Int, """block""" b: Int, c: Int): Int }\n',
  "operation-desc-long": '"""\nThe query\n"""\nscalar S\n',
};
for (const [k, v] of Object.entries(snippets)) for (const ext of [".graphql", ".gql"]) push(`snippet-${k}${ext}`, `snippet/${k}${ext}`, v);
for (const [k, v] of Object.entries(snippets)) push(`snippet-${k}-w40`, `snippet/${k}.graphql`, v, ["--print-width", "40"]);
fs.writeFileSync(out, lines.join("\n") + "\n"); console.log({ fixtures: texts.length, total: lines.length });
