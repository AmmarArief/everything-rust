import fs from "node:fs"; import path from "node:path";
const [root, inputs] = process.argv.slice(2);
const prettier = await import(path.join(root, "dist/prettier/index.mjs"));
const files = []; const counts = new Map();
for (const line of fs.readFileSync(inputs, "utf8").split("\n")) {
  if (!line) continue; const c = JSON.parse(line);
  const fp = c.args[c.args.indexOf("--stdin-filepath") + 1];
  const opts = c.args.filter(a => a !== "--stdin-filepath" && a !== fp);
  let ast; try { ({ ast } = await prettier.__debug.parse(c.stdin, { filepath: fp })); } catch { continue; }
  if (ast.comments?.length) continue;
  const types = new Set();
  const walk = n => { if (Array.isArray(n)) return n.forEach(walk); if (!n || typeof n !== "object") return;
    if (typeof n.type === "string") { let t = n.type;
      if (t === "Literal") t += ":" + (n.regex ? "regex" : n.bigint ? "bigint" : typeof n.value);
      if (t === "BinaryExpression" || t === "LogicalExpression" || t === "AssignmentExpression" || t === "UnaryExpression" || t === "UpdateExpression") t += ":" + n.operator;
      if (t === "VariableDeclaration") t += ":" + n.kind;
      types.add(t); }
    for (const [k, v] of Object.entries(n)) if (!["loc","range","start","end","extra","tokens","comments"].includes(k)) walk(v); };
  walk(ast.program ?? ast);
  files.push({ name: c.name, types: [...types], opts: opts.length > 0, size: c.stdin.length });
  for (const t of types) counts.set(t, (counts.get(t) ?? 0) + 1);
}
fs.writeFileSync(process.argv[4], JSON.stringify(files));
console.log("comment-free parsed cases", files.length);
console.log([...counts].sort((a,b)=>b[1]-a[1]).slice(0, 70).map(([t,n])=>`${n} ${t}`).join("\n"));
