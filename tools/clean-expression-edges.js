import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';

const require = createRequire(import.meta.url);
const sharp = require('sharp');

const inputDir = process.argv[2];
const outputDir = process.argv[3];

if (!inputDir || !outputDir) {
  console.error('Usage: node clean-expression-edges.js <input-dir> <output-dir>');
  process.exit(2);
}

const EDGE_RGB = [12, 20, 48];
const MAX_PEEL_PASSES = 12;
const NEIGHBORS = [
  [-1, -1], [0, -1], [1, -1],
  [-1, 0],            [1, 0],
  [-1, 1],  [0, 1],   [1, 1],
];

function isLightContamination(r, g, b) {
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const spread = max - min;
  const luminance = 0.2126 * r + 0.7152 * g + 0.0722 * b;
  return luminance > 112 || min > 78 || (luminance > 92 && spread < 52);
}

function touchesTransparency(data, width, height, x, y) {
  for (const [dx, dy] of NEIGHBORS) {
    const nx = x + dx;
    const ny = y + dy;
    if (nx < 0 || ny < 0 || nx >= width || ny >= height) return true;
    if (data[(ny * width + nx) * 4 + 3] === 0) return true;
  }
  return false;
}

async function cleanFile(inputPath, outputPath) {
  const { data, info } = await sharp(inputPath)
    .ensureAlpha()
    .raw()
    .toBuffer({ resolveWithObject: true });
  const { width, height } = info;

  let removed = 0;
  let passesUsed = 0;
  const removedByPass = [];

  // Normalize to hard alpha first and clear hidden RGB.
  for (let i = 0; i < data.length; i += 4) {
    if (data[i + 3] < 128) {
      data[i] = 0;
      data[i + 1] = 0;
      data[i + 2] = 0;
      data[i + 3] = 0;
    } else {
      data[i + 3] = 255;
    }
  }

  // Peel only light pixels exposed to transparency. Repeating reaches through
  // a several-pixel white matte while stopping at the darker real outline.
  for (let pass = 0; pass < MAX_PEEL_PASSES; pass += 1) {
    const peel = [];
    for (let y = 0; y < height; y += 1) {
      for (let x = 0; x < width; x += 1) {
        const i = (y * width + x) * 4;
        if (data[i + 3] === 0) continue;
        if (!touchesTransparency(data, width, height, x, y)) continue;
        if (isLightContamination(data[i], data[i + 1], data[i + 2])) peel.push(i);
      }
    }
    if (peel.length === 0) break;
    for (const i of peel) {
      data[i] = 0;
      data[i + 1] = 0;
      data[i + 2] = 0;
      data[i + 3] = 0;
    }
    removed += peel.length;
    passesUsed = pass + 1;
    removedByPass.push(peel.length);
  }

  // Replace the final one-pixel exposed silhouette with a dark navy outline.
  const outline = [];
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      const i = (y * width + x) * 4;
      if (data[i + 3] !== 0 && touchesTransparency(data, width, height, x, y)) outline.push(i);
    }
  }
  for (const i of outline) {
    data[i] = EDGE_RGB[0];
    data[i + 1] = EDGE_RGB[1];
    data[i + 2] = EDGE_RGB[2];
    data[i + 3] = 255;
  }

  await sharp(data, { raw: { width, height, channels: 4 } }).png().toFile(outputPath);
  return { width, height, removed, passesUsed, removedByPass, outlined: outline.length };
}

async function makeContactSheet(files, results, outputPath, background) {
  const thumbWidth = 260;
  const labelHeight = 38;
  const columns = 4;
  const rows = Math.ceil(files.length / columns);
  const cellWidth = thumbWidth + 24;
  const cellHeight = 410;
  const canvasWidth = columns * cellWidth;
  const canvasHeight = rows * cellHeight;
  const composites = [];

  for (let index = 0; index < files.length; index += 1) {
    const file = files[index];
    const left = (index % columns) * cellWidth + 12;
    const top = Math.floor(index / columns) * cellHeight + labelHeight;
    const thumb = await sharp(path.join(outputDir, file))
      .resize({ width: thumbWidth, height: cellHeight - labelHeight - 14, fit: 'contain' })
      .png()
      .toBuffer();
    composites.push({ input: thumb, left, top });

    const report = results[file];
    const textColor = background === '#000000' ? '#FFFFFF' : '#111111';
    const label = Buffer.from(
      `<svg width="${cellWidth}" height="${labelHeight}"><text x="12" y="24" fill="${textColor}" font-family="Segoe UI,Arial" font-size="16">${file.replace(/&/g, '&amp;')}  -${report.removed}px</text></svg>`,
    );
    composites.push({ input: label, left: (index % columns) * cellWidth, top: Math.floor(index / columns) * cellHeight });
  }

  await sharp({ create: { width: canvasWidth, height: canvasHeight, channels: 4, background } })
    .composite(composites)
    .png()
    .toFile(outputPath);
}

(async () => {
  fs.mkdirSync(outputDir, { recursive: true });
  const files = fs.readdirSync(inputDir).filter((name) => name.toLowerCase().endsWith('.png')).sort();
  if (files.length === 0) throw new Error(`No PNG files found in ${inputDir}`);

  const results = {};
  for (const file of files) {
    results[file] = await cleanFile(path.join(inputDir, file), path.join(outputDir, file));
  }

  await makeContactSheet(files, results, path.join(outputDir, '_qa-black.png'), '#000000');
  await makeContactSheet(files, results, path.join(outputDir, '_qa-white.png'), '#FFFFFF');
  fs.writeFileSync(
    path.join(outputDir, '_edge-clean-report.json'),
    `${JSON.stringify({ algorithm: 'hard-edge-white-matte-peel-v1', edgeRgb: EDGE_RGB, maxPeelPasses: MAX_PEEL_PASSES, files: results }, null, 2)}\n`,
    'utf8',
  );
  console.log(JSON.stringify(results, null, 2));
})().catch((error) => {
  console.error(error.stack || error.message || String(error));
  process.exit(1);
});
