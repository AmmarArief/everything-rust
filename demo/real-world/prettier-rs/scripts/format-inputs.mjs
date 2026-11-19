import fs from "node:fs";
const [inputs, claimed, out] = process.argv.slice(2);
const want = new Set(fs.readFileSync(claimed, "utf8").split("\n").filter(Boolean));
const flag = (args) => { const o = {}; for (let i = 0; i < args.length; i++) { const a = args[i]; const v = () => args[++i];
  switch (a) { case "--stdin-filepath": o.filepath = v(); break; case "--no-semi": o.semi = false; break; case "--single-quote": o.singleQuote = true; break;
  case "--trailing-comma": o.trailingComma = v(); break; case "--arrow-parens": o.arrowParens = v(); break; case "--print-width": o.printWidth = +v(); break;
  case "--use-tabs": o.useTabs = true; break; case "--tab-width": o.tabWidth = +v(); break; case "--no-bracket-spacing": o.bracketSpacing = false; break;
  case "--quote-props": o.quoteProps = v(); break; case "--jsx-single-quote": o.jsxSingleQuote = true; break; case "--bracket-same-line": o.bracketSameLine = true; break;
  case "--object-wrap": o.objectWrap = v(); break; case "--experimental-ternaries": o.experimentalTernaries = true; break;
  case "--experimental-operator-position": o.experimentalOperatorPosition = v(); break; case "--end-of-line": o.endOfLine = v(); break; case "--parser": o.parser = v(); break; case "--prose-wrap": o.proseWrap = v(); break; default: throw new Error(a); } } return o; };
const lines = []; let found = 0;
for (const l of fs.readFileSync(inputs, "utf8").split("\n")) { if (!l) continue; const c = JSON.parse(l);
  if (!want.has(c.name)) continue; found++; lines.push(JSON.stringify({ fn: "format", args: [c.stdin, flag(c.args)] })); }
fs.writeFileSync(out, lines.join("\n") + "\n"); console.log({ claimed: want.size, found });
