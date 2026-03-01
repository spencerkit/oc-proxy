const fs = require("node:fs");
const path = require("node:path");

const root = path.resolve(__dirname, "..");
const srcProxy = path.join(root, "src", "proxy");
const outProxy = path.join(root, "out", "proxy");

if (!fs.existsSync(srcProxy)) {
  throw new Error(`Source proxy directory not found: ${srcProxy}`);
}

fs.rmSync(outProxy, { recursive: true, force: true });
fs.mkdirSync(path.dirname(outProxy), { recursive: true });
fs.cpSync(srcProxy, outProxy, { recursive: true });

console.log(`[sync-proxy] synced ${srcProxy} -> ${outProxy}`);
