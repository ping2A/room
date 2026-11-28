#!/usr/bin/env node
// Compile an AssemblyScript Room mod exactly like the web editor does.
//
//   ./scripts/fetch-assemblyscript.sh            # once (npm install)
//   node scripts/asc-compile.mjs mod.ts mod.wasm
//
// `mod.ts` imports from "./room" (crates/room-web/as/room.ts is provided).
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const [input, output] = process.argv.slice(2);
if (!input || !output) {
  console.error("usage: node scripts/asc-compile.mjs <mod.ts> <out.wasm>");
  process.exit(2);
}
const ascPath = path.join(root, "target/asc-npm/node_modules/assemblyscript/dist/asc.js");
if (!fs.existsSync(ascPath)) {
  console.error("AssemblyScript not installed: run ./scripts/fetch-assemblyscript.sh");
  process.exit(1);
}
const asc = (await import(pathToFileURL(ascPath).href)).default;

const lib = path.join(root, "crates/room-web/as");
let source = fs.readFileSync(input, "utf8");
if (!/export\s*\*\s*from\s*["']\.\/room_abi["']/.test(source)) source += '\nexport * from "./room_abi";\n';
const files = {
  "input.ts": source,
  "room.ts": fs.readFileSync(path.join(lib, "room.ts"), "utf8"),
  "room_abi.ts": fs.readFileSync(path.join(lib, "room_abi.ts"), "utf8"),
};
const out = {};
const res = await asc.main(
  ["input.ts", "--runtime", "incremental", "--use", "abort=room/roomAbort", "-O2", "--noColors", "-o", "out.wasm"],
  { readFile: (n) => files[n] ?? null, writeFile: (n, d) => { out[n] = d; }, listFiles: () => [] },
);
process.stderr.write(res.stderr.toString());
if (res.error || !out["out.wasm"]) process.exit(1);
fs.writeFileSync(output, out["out.wasm"]);
console.log(`${output}: ${out["out.wasm"].length} bytes`);
