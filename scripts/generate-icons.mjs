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
const kit = path.join(root, 'output/brand/plume');
const dark = fs.readFileSync(path.join(native, 'plume-app.svg'), 'utf8');
const light = dark.replace('fill="#000"', 'fill="#fff"').replace('<path fill="#fff"', '<path fill="#000"');
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

fs.writeFileSync(path.join(kit, 'icon-dark.svg'), dark);
fs.writeFileSync(path.join(kit, 'icon-light.svg'), light);
for (const size of [16, 24, 32, 48, 64, 128, 180, 192, 256, 512, 1024]) {
  const data = await png(dark, size);
  // 24px is only used inside the Windows ICO.
  if (size !== 24) fs.writeFileSync(path.join(kit, `icon-${size}.png`), data);
}
fs.writeFileSync(path.join(kit, 'icon-dark.png'), await png(dark, 1280));
fs.writeFileSync(path.join(kit, 'icon-light.png'), await png(light, 1280));
fs.writeFileSync(path.join(native, 'plume.png'), await png(dark, 1024));
fs.writeFileSync(path.join(native, 'plume-linux.png'), await png(dark, 512));
fs.writeFileSync(path.join(native, 'plume-window.rgba'), await render(dark, 128).ensureAlpha().raw().toBuffer());
const representations = async sizes => Promise.all(sizes.map(async size => ({ size, data: await png(dark, size) })));
fs.writeFileSync(path.join(native, 'plume.ico'), ico(await representations([16, 24, 32, 48, 64, 128, 256])));
fs.writeFileSync(path.join(kit, 'favicon.ico'), ico(await representations([16, 32, 48, 256])));

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
    fs.copyFileSync(path.join(native, 'plume.icns'), path.join(kit, 'plume.icns'));
  } finally {
    fs.rmSync(temporary, { recursive: true, force: true });
  }
} else {
  console.log('Run on macOS to also refresh the ICNS file.');
}

const panels = [
  ['mark-white.svg', '#181818', 'White mark'],
  ['mark-black.svg', '#eeeeee', 'Black mark'],
  ['icon-dark.svg', '#eeeeee', 'Dark app icon'],
  ['icon-light.svg', '#181818', 'Light app icon'],
  ['wordmark-white.svg', '#181818', 'White wordmark'],
  ['wordmark-black.svg', '#eeeeee', 'Black wordmark'],
];
const layers = [];
for (const [i, [filename, background, label]] of panels.entries()) {
  const width = 600, height = 480;
  const ink = background === '#181818' ? '#fff' : '#000';
  const panel = Buffer.from(`<svg width="${width}" height="${height}" xmlns="http://www.w3.org/2000/svg"><rect width="${width}" height="${height}" fill="${background}"/><text x="30" y="440" font-family="Arial,Helvetica,sans-serif" font-size="24" fill="${ink}">${label}</text></svg>`);
  const wordmark = filename.startsWith('wordmark');
  const artwork = await sharp(path.join(kit, filename), { density: 144 }).resize(wordmark ? 500 : 310, wordmark ? 160 : 310, { fit: 'contain', background: '#0000' }).png().toBuffer();
  const composite = await sharp(panel).composite([{ input: artwork, left: wordmark ? 50 : 145, top: wordmark ? 145 : 50 }]).png().toBuffer();
  layers.push({ input: composite, left: (i % 3) * width, top: Math.floor(i / 3) * height });
}
await sharp({ create: { width: 1800, height: 960, channels: 4, background: '#eee' } }).composite(layers).png().toFile(path.join(kit, 'preview.png'));
console.log('Rounded application icons generated for macOS, Windows, Linux and the brand kit.');
