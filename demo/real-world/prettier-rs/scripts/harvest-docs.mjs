import fs from "node:fs"; import path from "node:path";
const [root, inputs, out] = process.argv.slice(2);
const prettier = await import(path.join(root, "dist/prettier/index.mjs"));
const flagToOpt = (args) => { const o = {}; for (let i = 0; i < args.length; i++) { const a = args[i];
  const next = () => args[++i];
  switch (a) {
    case "--stdin-filepath": o.filepath = next(); break;
    case "--no-semi": o.semi = false; break; case "--single-quote": o.singleQuote = true; break;
    case "--trailing-comma": o.trailingComma = next(); break; case "--arrow-parens": o.arrowParens = next(); break;
    case "--print-width": o.printWidth = Number(next()); break; case "--use-tabs": o.useTabs = true; break;
    case "--tab-width": o.tabWidth = Number(next()); break; case "--no-bracket-spacing": o.bracketSpacing = false; break;
    case "--quote-props": o.quoteProps = next(); break; case "--jsx-single-quote": o.jsxSingleQuote = true; break;
    case "--bracket-same-line": o.bracketSameLine = true; break; case "--object-wrap": o.objectWrap = next(); break;
    case "--experimental-ternaries": o.experimentalTernaries = true; break;
    case "--experimental-operator-position": o.experimentalOperatorPosition = next(); break;
    default: throw new Error("flag " + a);
  } } return o; };
function propagateBreaks(doc) {
  const visited = new Set(); const stack = [];
  const breakParent = () => { const g = stack.at(-1); if (g && !g.expandedStates && !g.break) g.break = "propagated"; };
  prettier.doc.utils.traverseDoc(doc, d => {
    if (d.type === "break-parent") breakParent();
    if (d.type === "group") { stack.push(d); if (visited.has(d)) return false; visited.add(d); }
  }, d => { if (d.type === "group" && stack.pop().break) breakParent(); }, true);
}
function toJson(doc) {
  const ids = new Map(); const id = s => { if (!ids.has(s)) ids.set(s, `g${ids.size}`); return ids.get(s); };
  let budget = 300000;
  const rec = d => {
    if (--budget < 0) throw new Error("too big");
    if (typeof d === "string") return d;
    if (Array.isArray(d)) return d.map(rec);
    const o = {};
    for (const [k, v] of Object.entries(d)) {
      if (typeof v === "symbol") o[k] = id(v);
      else if (k === "n" && typeof v === "number" && !Number.isFinite(v)) o[k] = { $num: v > 0 ? "Infinity" : "-Infinity" };
      else if (k === "n" && v && typeof v === "object") o[k] = { ...v };
      else if (v && typeof v === "object") o[k] = Array.isArray(v) ? v.map(rec) : rec(v);
      else o[k] = v;
    }
    return o;
  };
  return rec(doc);
}
const fd = fs.openSync(out, "w"); let n = 0, skipped = 0, big = 0, bytes = 0;
const wanted = /^(js|jsx|typescript)-|^print-width|^use-tabs|^no-semi|^single-quote|^trailing-comma|^real-/;
for (const line of fs.readFileSync(inputs, "utf8").split("\n")) {
  if (!line) continue; const c = JSON.parse(line); if (!wanted.test(c.name)) continue;
  const o = flagToOpt(c.args);
  let doc; try { doc = await prettier.__debug.printToDoc(c.stdin, o); } catch { skipped++; continue; }
  const opts = { printWidth: o.printWidth ?? 80, tabWidth: o.tabWidth ?? 2, useTabs: o.useTabs ?? false, endOfLine: "lf" };
  propagateBreaks(doc);
  let j; try { j = toJson(doc); } catch { big++; continue; }
  const s = JSON.stringify({ fn: "printDocToString", args: [j, opts] });
  bytes += s.length; fs.writeSync(fd, s + "\n"); n++;
}
fs.closeSync(fd);
console.log({ n, skipped, big, mb: (bytes / 1e6).toFixed(1) });
