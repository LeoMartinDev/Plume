// Regenerate native icons from the approved vector mark. Requires sharp;
// macOS iconutil also generates the standard/Retina ICNS representations.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';

const sharp = createRequire(import.meta.url)('sharp');
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const native = path.join(root, 'crates/plume-app/assets/brand');
const dark = fs.readFileSync(path.join(native, 'plume-app.svg'), 'utf8');
const menubar = fs.readFileSync(path.join(native, 'plume-menubar.svg'), 'utf8');
const render = (svg, size) => sharp(Buffer.from(svg), { density: 144 }).resize(size, size);
const png = (svg, size) => render(svg, size).png().toBuffer();

function ico(images) {
  const directory = Buffer.alloc(6 + images.length * 16);
  directory.writeUInt16LE(1, 2);
  directory.writeUInt16LE(images.length, 4);
  let offset = directory.length;
  images.forEach(({ size, data }, i) => {
    const entry = 6 + i * 16;
    directory[entry] = directory[entry + 1] = size === 256 ? 0 : size;
    directory.writeUInt16LE(1, entry + 4);
    directory.writeUInt16LE(32, entry + 6);
    directory.writeUInt32LE(data.length, entry + 8);
    directory.writeUInt32LE(offset, entry + 12);
    offset += data.length;
  });
  return Buffer.concat([directory, ...images.map(image => image.data)]);
}

fs.writeFileSync(path.join(native, 'plume.png'), await png(dark, 1024));
fs.writeFileSync(path.join(native, 'plume-menubar.png'), await png(menubar, 64));
fs.writeFileSync(path.join(native, 'plume-linux.png'), await png(dark, 512));
fs.writeFileSync(path.join(native, 'plume-window.rgba'), await render(dark, 128).ensureAlpha().raw().toBuffer());
const representations = async sizes => Promise.all(sizes.map(async size => ({ size, data: await png(dark, size) })));
fs.writeFileSync(path.join(native, 'plume.ico'), ico(await representations([16, 24, 32, 48, 64, 128, 256])));

if (process.platform === 'darwin') {
  const temporary = fs.mkdtempSync(path.join(os.tmpdir(), 'plume-icons-'));
  try {
    const iconset = path.join(temporary, 'plume.iconset');
    fs.mkdirSync(iconset);
    for (const size of [16, 32, 128, 256, 512]) {
      fs.writeFileSync(path.join(iconset, `icon_${size}x${size}.png`), await png(dark, size));
      fs.writeFileSync(path.join(iconset, `icon_${size}x${size}@2x.png`), await png(dark, size * 2));
    }
    execFileSync('iconutil', ['--convert', 'icns', '--output', path.join(native, 'plume.icns'), iconset]);
  } finally {
    fs.rmSync(temporary, { recursive: true, force: true });
  }
} else {
  console.log('Run on macOS to also refresh the ICNS file.');
}

console.log('Native application icons generated for macOS, Windows and Linux.');
