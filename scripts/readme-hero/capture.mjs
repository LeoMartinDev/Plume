// Record the README hero (scene.html) as seamless animated WebP loops, light and dark.
// node scripts/readme-hero/capture.mjs [light|dark ...] [--keep-frames]
// Needs ffmpeg built with libwebp, and Chrome or Edge (or CHROME=/path/to/browser).
// The browser is driven over the DevTools protocol: no npm dependency.
// app-light.png and app-dark.png are the onboarding window's client area, captured from
// cargo run -p plume-app --example onboarding_preview -- model --light (then --dark).
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawn, execFileSync } from 'node:child_process';
import { fileURLToPath, pathToFileURL } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '../..');
const output = path.join(root, 'docs/images');

const WIDTH = 1280;
const HEIGHT = 760;
const SCALE = 2;
const LOOP_MS = 8000;
const FPS = 25;
const OUT_WIDTH = 1600;
const PORT = 9333;
const args = process.argv.slice(2);
const keepFrames = args.includes('--keep-frames');
const themes = args.filter((arg) => !arg.startsWith('--'));

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

async function record(page, theme) {
  const frames = fs.mkdtempSync(path.join(os.tmpdir(), `plume-hero-${theme}-`));
  const scene = pathToFileURL(path.join(here, 'scene.html')).href + `?theme=${theme}`;
  await page.send('Page.navigate', { url: scene });
  await evaluate(page, 'new Promise((r) => (document.readyState === "complete" ? r() : addEventListener("load", r)))');
  await evaluate(page, 'document.fonts.ready.then(() => document.fonts.size)');
  await evaluate(page, 'document.getElementById("app").decode().then(() => true)');
  const count = (LOOP_MS / 1000) * FPS;
  for (let i = 0; i < count; i++) {
    await evaluate(page, `render(${(i * 1000) / FPS})`);
    const { data } = await page.send('Page.captureScreenshot', { format: 'png' });
    fs.writeFileSync(path.join(frames, `${String(i).padStart(4, '0')}.png`), Buffer.from(data, 'base64'));
    process.stdout.write(`\r${theme}: frame ${i + 1}/${count}`);
  }
  process.stdout.write('\n');

  const target = path.join(output, `plume-hero-${theme}.webp`);
  execFileSync(
    'ffmpeg',
    [
      '-y', '-loglevel', 'error',
      '-framerate', String(FPS),
      '-i', path.join(frames, '%04d.png'),
      '-vf', `scale=${OUT_WIDTH}:-1:flags=lanczos`,
      '-c:v', 'libwebp_anim',
      '-lossless', '0',
      '-quality', '82',
      '-compression_level', '6',
      '-preset', 'picture',
      '-loop', '0',
      '-an',
      target,
    ],
    { stdio: 'inherit' },
  );
  if (keepFrames) console.log(`${theme} frames kept in ${frames}`);
  else fs.rmSync(frames, { recursive: true, force: true });
  console.log(`${path.relative(root, target)}: ${(fs.statSync(target).size / 1024 / 1024).toFixed(2)} MB`);
}

const profile = fs.mkdtempSync(path.join(os.tmpdir(), 'plume-hero-browser-'));
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
  await page.send('Emulation.setDeviceMetricsOverride', {
    width: WIDTH,
    height: HEIGHT,
    deviceScaleFactor: SCALE,
    mobile: false,
  });
  fs.mkdirSync(output, { recursive: true });
  for (const theme of themes.length ? themes : ['light', 'dark']) {
    await record(page, theme);
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
