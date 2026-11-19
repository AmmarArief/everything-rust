import fs from "node:fs";
import { spawnSync } from "node:child_process";
const [prettierCli, frontDoor, inputs, names] = process.argv.slice(2);
const want = new Set(fs.readFileSync(names, "utf8").split("\n").filter(Boolean));
const cases = fs.readFileSync(inputs, "utf8").split("\n").filter(Boolean).map((l) => JSON.parse(l)).filter((c) => want.has(c.name));
const run = (cmd, args, c) => {
  const start = process.hrtime.bigint();
  const r = spawnSync(cmd, [...args, ...c.args], { input: c.stdin, timeout: 60_000 });
  const ms = Number(process.hrtime.bigint() - start) / 1e6;
  if (r.status === null) throw new Error(`timeout ${c.name}`);
  return { ms, out: r.stdout.toString() };
};
let node = 0, rust = 0, same = 0;
for (const c of cases) {
  const a = run(process.execPath, [prettierCli], c);
  const b = run(frontDoor, [], c);
  node += a.ms; rust += b.ms; if (a.out === b.out) same++;
}
console.log(JSON.stringify({ cases: cases.length, identical: same, prettier_ms: Math.round(node), front_door_ms: Math.round(rust) }));
