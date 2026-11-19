import path from "node:path"; import { pathToFileURL } from "node:url";
const [dir, filepath] = process.argv.slice(2);
const prettier = await import(pathToFileURL(path.join(dir, "index.mjs")).href);
let input = ""; for await (const c of process.stdin) input += c;
try { process.stdout.write(await prettier.format(input, { filepath })); } catch { process.exit(2); }
