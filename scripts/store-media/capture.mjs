// Render the store media (scene.html): one PNG per scene and a promo video, light and dark.
// node scripts/store-media/capture.mjs [stills] [video] [light|dark ...] [--scene=name] [--keep-frames]
// Needs ffmpeg built with libx264, and Chrome or Edge (or CHROME=/path/to/browser).
// The browser is driven over the DevTools protocol: no npm dependency.
// The Plume windows in app/ are client-area captures of
// cargo run -p plume-app --example settings_preview -- <dir> (seeded history, Ctrl+Space shortcuts)
// and cargo run -p plume-app --example onboarding_preview -- <model|shortcuts> --light (then --dark).
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawn, execFileSync } from 'node:child_process';
import { fileURLToPath, pathToFileURL } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '../..');
const output = path.join(root, 'docs/images/store');

const WIDTH = 1280;
const HEIGHT = 720;
const STILL_SCALE = 1.5; // 1920×1080
const VIDEO_SCALE = 1.5; // 1920×1080
const FPS = 30;
const VIDEO_START = 2000; // skip the opening fade, where the stage flashes grey
const PORT = 9334;
const args = process.argv.slice(2);
const keepFrames = args.includes('--keep-frames');
const only = args.find((arg) => arg.startsWith('--scene='))?.slice(8);
const words = args.filter((arg) => !arg.startsWith('--'));
const themes = words.filter((w) => w === 'light' || w === 'dark');
const kinds = words.filter((w) => w === 'stills' || w === 'video');

function browser() {
  const candidates = [
    process.env.CHROME,
    'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe',
    'C:/Program Files/Google/Chrome/Application/chrome.exe',
    '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
    '/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge',
    '/usr/bin/google-chrome',
    '/usr/bin/chromium',
  ];
  const found = candidates.find((candidate) => candidate && fs.existsSync(candidate));
  if (!found) throw new Error('No Chrome or Edge found; set CHROME=/path/to/browser');
  return found;
}

async function connect() {
  for (let attempt = 0; attempt < 50; attempt++) {
    try {
      const targets = await (await fetch(`http://127.0.0.1:${PORT}/json/list`)).json();
      const page = targets.find((target) => target.type === 'page');
      if (page) return page.webSocketDebuggerUrl;
    } catch {
      // The browser is still starting.
    }
    await new Promise((resolve) => setTimeout(resolve, 200));
  }
  throw new Error('The browser did not expose a page over DevTools');
}

function session(url) {
  const socket = new WebSocket(url);
  let next = 0;
  const pending = new Map();
  socket.addEventListener('message', (event) => {
    const message = JSON.parse(event.data);
    const call = pending.get(message.id);
    if (!call) return;
    pending.delete(message.id);
    if (message.error) call.reject(new Error(message.error.message));
    else call.resolve(message.result);
  });
  const opened = new Promise((resolve, reject) => {
    socket.addEventListener('open', resolve, { once: true });
    socket.addEventListener('error', reject, { once: true });
  });
  return {
    opened,
    send(method, params = {}) {
      const id = ++next;
      socket.send(JSON.stringify({ id, method, params }));
      return new Promise((resolve, reject) => pending.set(id, { resolve, reject }));
    },
    close: () => socket.close(),
  };
}

async function evaluate(page, expression) {
  const { result, exceptionDetails } = await page.send('Runtime.evaluate', {
    expression,
    awaitPromise: true,
    returnByValue: true,
  });
  if (exceptionDetails) throw new Error(exceptionDetails.exception?.description ?? expression);
  return result.value;
}

async function open(page, theme, scale) {
  await page.send('Emulation.setDeviceMetricsOverride', {
    width: WIDTH,
    height: HEIGHT,
    deviceScaleFactor: scale,
    mobile: false,
  });
  const scene = pathToFileURL(path.join(here, 'scene.html')).href + `?theme=${theme}`;
  await page.send('Page.navigate', { url: scene });
  await evaluate(page, 'new Promise((r) => (document.readyState === "complete" ? r() : addEventListener("load", r)))');
  await evaluate(page, 'document.fonts.ready.then(() => document.fonts.size)');
  await evaluate(page, 'Promise.all([...document.images].map((i) => i.decode())).then(() => true)');
}

async function shoot(page) {
  const { data } = await page.send('Page.captureScreenshot', { format: 'png' });
  return Buffer.from(data, 'base64');
}

async function stills(page, theme) {
  await open(page, theme, STILL_SCALE);
  const names = await evaluate(page, 'window.SCENES');
  let index = 0;
  for (const name of names) {
    index++;
    if (only && name !== only) continue;
    await evaluate(page, `render(0, ${JSON.stringify(name)})`);
    const target = path.join(output, `plume-${String(index).padStart(2, '0')}-${name}-${theme}.png`);
    fs.writeFileSync(target, await shoot(page));
    console.log(path.relative(root, target));
  }
}

async function video(page, theme) {
  await open(page, theme, VIDEO_SCALE);
  const duration = await evaluate(page, 'window.DURATION');
  const frames = fs.mkdtempSync(path.join(os.tmpdir(), `plume-store-${theme}-`));
  const count = Math.round(((duration - VIDEO_START) / 1000) * FPS);
  for (let i = 0; i < count; i++) {
    await evaluate(page, `render(${VIDEO_START + (i * 1000) / FPS})`);
    fs.writeFileSync(path.join(frames, `${String(i).padStart(5, '0')}.png`), await shoot(page));
    process.stdout.write(`\r${theme}: frame ${i + 1}/${count}`);
  }
  process.stdout.write('\n');
  const target = path.join(output, `plume-promo-${theme}.mp4`);
  execFileSync(
    'ffmpeg',
    [
      '-y', '-loglevel', 'error',
      '-framerate', String(FPS),
      '-i', path.join(frames, '%05d.png'),
      '-c:v', 'libx264',
      '-preset', 'slow',
      '-crf', '16',
      '-pix_fmt', 'yuv420p',
      '-movflags', '+faststart',
      '-an',
      target,
    ],
    { stdio: 'inherit' },
  );
  if (keepFrames) console.log(`${theme} frames kept in ${frames}`);
  else fs.rmSync(frames, { recursive: true, force: true });
  console.log(`${path.relative(root, target)}: ${(fs.statSync(target).size / 1024 / 1024).toFixed(2)} MB`);
}

const profile = fs.mkdtempSync(path.join(os.tmpdir(), 'plume-store-browser-'));
const child = spawn(
  browser(),
  [
    '--headless=new',
    `--remote-debugging-port=${PORT}`,
    `--user-data-dir=${profile}`,
    '--allow-file-access-from-files',
    '--hide-scrollbars',
    '--force-color-profile=srgb',
    '--no-first-run',
    'about:blank',
  ],
  { stdio: 'ignore' },
);

try {
  const page = session(await connect());
  await page.opened;
  await page.send('Page.enable');
  await page.send('Runtime.enable');
  fs.mkdirSync(output, { recursive: true });
  for (const theme of themes.length ? themes : ['light', 'dark']) {
    if (!kinds.length || kinds.includes('stills')) await stills(page, theme);
    if (!kinds.length || kinds.includes('video')) await video(page, theme);
  }
  page.close();
} finally {
  // On Windows, killing the launcher leaves the renderer and GPU processes running.
  if (process.platform === 'win32') {
    try {
      execFileSync('taskkill', ['/pid', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
    } catch {}
  } else {
    child.kill();
  }
  // The browser may still hold its profile for a moment; a leftover temp folder is harmless.
  setTimeout(() => {
    try {
      fs.rmSync(profile, { recursive: true, force: true });
    } catch {}
  }, 1000);
}
