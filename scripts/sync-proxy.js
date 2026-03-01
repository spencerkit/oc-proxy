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

function walk(dir) {
  const entries = fs.readdirSync(dir, { withFileTypes: true });
  for (const entry of entries) {
    const fullPath = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      walk(fullPath);
      continue;
    }
    if (!entry.isFile() || !fullPath.endsWith(".ts")) {
      continue;
    }

    const tsContent = fs.readFileSync(fullPath, "utf-8");
    const jsContent = tsContent.replace(
      /(require\((["'`])[^"'`]+)\.ts\2\)/g,
      "$1.js$2)"
    );
    const jsPath = fullPath.slice(0, -3) + ".js";
    fs.writeFileSync(jsPath, jsContent, "utf-8");
  }
}

walk(outProxy);

console.log(`[sync-proxy] synced ${srcProxy} -> ${outProxy} (with .js mirrors)`);
