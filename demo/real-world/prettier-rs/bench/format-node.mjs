import fs from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";
const [dir, inputs, names, repsArg] = process.argv.slice(2);
const prettier = await import(pathToFileURL(path.join(dir, "index.mjs")).href);
const want = new Set(fs.readFileSync(names, "utf8").split("\n").filter(Boolean));
const reps = Number(repsArg ?? 20);
const flag = (args) => { const o = {}; for (let i = 0; i < args.length; i++) { const a = args[i]; const v = () => args[++i];
  switch (a) { case "--stdin-filepath": o.filepath = v(); break; case "--no-semi": o.semi = false; break; case "--single-quote": o.singleQuote = true; break;
  case "--trailing-comma": o.trailingComma = v(); break; case "--arrow-parens": o.arrowParens = v(); break; case "--print-width": o.printWidth = +v(); break;
  case "--use-tabs": o.useTabs = true; break; case "--tab-width": o.tabWidth = +v(); break; case "--no-bracket-spacing": o.bracketSpacing = false; break;
  case "--quote-props": o.quoteProps = v(); break; case "--jsx-single-quote": o.jsxSingleQuote = true; break; case "--bracket-same-line": o.bracketSameLine = true; break;
  case "--object-wrap": o.objectWrap = v(); break; case "--experimental-ternaries": o.experimentalTernaries = true; break;
  case "--experimental-operator-position": o.experimentalOperatorPosition = v(); break; case "--end-of-line": o.endOfLine = v(); break; case "--parser": o.parser = v(); break; case "--prose-wrap": o.proseWrap = v(); break; default: throw new Error(a); } } return o; };
let total = 0;
for (const line of fs.readFileSync(inputs, "utf8").split("\n")) {
  if (!line) continue;
  const c = JSON.parse(line);
  if (!want.has(c.name)) continue;
  const opts = flag(c.args);
  await prettier.format(c.stdin, opts);
  const start = process.hrtime.bigint();
  for (let i = 0; i < reps; i++) await prettier.format(c.stdin, opts);
  const us = Number(process.hrtime.bigint() - start) / 1000 / reps;
  total += us;
  console.log(`${c.name}\t${Buffer.byteLength(c.stdin)}\t${us.toFixed(1)}`);
}
console.error(`total_us\t${total.toFixed(0)}`);
