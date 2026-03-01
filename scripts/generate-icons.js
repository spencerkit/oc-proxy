const fs = require("node:fs");
const path = require("node:path");
const { app, nativeImage } = require("electron");

function ensureDir(targetPath) {
  fs.mkdirSync(path.dirname(targetPath), { recursive: true });
}

function toIcoFromPng(pngBuffer) {
  const headerSize = 6;
  const entrySize = 16;
  const dataOffset = headerSize + entrySize;
  const out = Buffer.alloc(dataOffset + pngBuffer.length);

  // ICONDIR
  out.writeUInt16LE(0, 0); // reserved
  out.writeUInt16LE(1, 2); // type: icon
  out.writeUInt16LE(1, 4); // image count

  // ICONDIRENTRY
  out.writeUInt8(0, 6); // width: 256
  out.writeUInt8(0, 7); // height: 256
  out.writeUInt8(0, 8); // color count
  out.writeUInt8(0, 9); // reserved
  out.writeUInt16LE(1, 10); // planes
  out.writeUInt16LE(32, 12); // bit count
  out.writeUInt32LE(pngBuffer.length, 14); // bytes in resource
  out.writeUInt32LE(dataOffset, 18); // image offset

  pngBuffer.copy(out, dataOffset);
  return out;
}

function toIcnsFromPngChunks(chunks) {
  const chunkBuffers = chunks.map(({ type, pngBuffer }) => {
    const chunk = Buffer.alloc(8 + pngBuffer.length);
    chunk.write(type, 0, 4, "ascii");
    chunk.writeUInt32BE(8 + pngBuffer.length, 4);
    pngBuffer.copy(chunk, 8);
    return chunk;
  });

  const totalSize = 8 + chunkBuffers.reduce((sum, chunk) => sum + chunk.length, 0);
  const out = Buffer.alloc(totalSize);
  out.write("icns", 0, 4, "ascii");
  out.writeUInt32BE(totalSize, 4);

  let offset = 8;
  for (const chunk of chunkBuffers) {
    chunk.copy(out, offset);
    offset += chunk.length;
  }

  return out;
}

async function run() {
  const rootDir = path.resolve(__dirname, "..");
  const sourceJpg = path.join(rootDir, "assets", "icon.jpg");
  const targetPng = path.join(rootDir, "assets", "icon.png");
  const targetIco = path.join(rootDir, "assets", "icon.ico");
  const targetIcns = path.join(rootDir, "assets", "icon.icns");

  if (!fs.existsSync(sourceJpg)) {
    throw new Error(`Source icon not found: ${sourceJpg}`);
  }

  const srcImage = nativeImage.createFromPath(sourceJpg);
  if (srcImage.isEmpty()) {
    throw new Error(`Failed to load source icon: ${sourceJpg}`);
  }

  const pngImage = srcImage.resize({ width: 512, height: 512, quality: "best" });
  const pngBuffer = pngImage.toPNG();

  ensureDir(targetPng);
  fs.writeFileSync(targetPng, pngBuffer);

  const icoPng = srcImage.resize({ width: 256, height: 256, quality: "best" }).toPNG();
  const icoBuffer = toIcoFromPng(icoPng);
  fs.writeFileSync(targetIco, icoBuffer);

  const icnsBuffer = toIcnsFromPngChunks([
    { type: "ic08", pngBuffer: icoPng },
    { type: "ic09", pngBuffer }
  ]);
  fs.writeFileSync(targetIcns, icnsBuffer);

  console.log(`Generated: ${path.relative(rootDir, targetPng)}`);
  console.log(`Generated: ${path.relative(rootDir, targetIco)}`);
  console.log(`Generated: ${path.relative(rootDir, targetIcns)}`);
}

app.whenReady().then(async () => {
  try {
    await run();
    app.exit(0);
  } catch (error) {
    console.error(error);
    app.exit(1);
  }
});
