import fs from "node:fs"; import path from "node:path";
const root = process.argv[2];
const prettier = await import(path.join(root, "dist/prettier/index.mjs"));
const oxc = await import(path.join(root, "dist/plugin-oxc/index.mjs"));
const files = [];
const walk = d => { for (const e of fs.readdirSync(d, {withFileTypes:true})) { const p = path.join(d, e.name);
  if (e.isDirectory()) { if (e.name !== "__snapshots__") walk(p); }
  else if (/\.(js|jsx|ts|tsx|mjs|cjs|mts|cts)$/.test(e.name) && e.name !== "format.test.js") files.push(p); } };
for (const d of ["js","jsx","typescript"]) walk(path.join(root, "tests/format", d));
files.sort();
let ok=0, err=0, withComments=0, oxcSame=0, oxcDiff=0, oxcErr=0, bytes=0;
for (const f of files) {
  const text = fs.readFileSync(f, "utf8"); const ts = /\.[mc]?tsx?$/.test(f);
  const parser = ts ? "typescript" : "babel";
  let out; try { out = await prettier.format(text, {parser}); ok++; bytes += text.length; } catch { err++; continue; }
  try { const {ast} = await prettier.__debug.parse(text, {parser}); if (ast.comments?.length) withComments++; } catch {}
  try { const o = await prettier.format(text, {parser: ts ? "oxc-ts" : "oxc", plugins:[oxc]}); o === out ? oxcSame++ : oxcDiff++; } catch { oxcErr++; }
}
console.log(JSON.stringify({files: files.length, ok, err, withComments, oxcSame, oxcDiff, oxcErr, bytes}));
