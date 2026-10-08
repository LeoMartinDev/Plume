import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { buildInstaller, extractInstaller, installerName, targets } from './installer.mjs';

test('native release names are installers on all supported targets', () => {
  assert.deepEqual(targets.map(target => installerName('1.2.3', target)), [
    'Plume-v1.2.3-x86_64-unknown-linux-gnu.deb',
    'Plume-v1.2.3-x86_64-pc-windows-msvc.exe',
    'Plume-v1.2.3-aarch64-apple-darwin.dmg',
  ]);
  assert.throws(() => installerName('1.2.3', 'unsupported'));
});

test('macOS disk image stages only the app and Applications shortcut and saves a Finder layout', t => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'plume-dmg-test-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const stage = path.join(root, 'stage');
  const dist = path.join(root, 'dist');
  fs.mkdirSync(path.join(stage, 'Plume.app/Contents/MacOS'), { recursive: true });
  fs.mkdirSync(dist);
  fs.writeFileSync(path.join(stage, 'Plume.app/Contents/MacOS/plume'), 'app');
  fs.writeFileSync(path.join(stage, 'plume'), 'duplicate outside bundle');
  const calls = [];
  buildInstaller({ platform: 'darwin', version: '1.2.3', target: targets[2], stage, dist, run: (...args) => calls.push(args) });
  assert.deepEqual(fs.readdirSync(path.join(dist, 'dmg-root')), ['.DS_Store', 'Applications', 'Plume.app']);
  // Windows resolves a root-relative symlink against the current drive.
  assert.equal(fs.readlinkSync(path.join(dist, 'dmg-root/Applications')), path.resolve('/Applications'));
  assert.equal(fs.readFileSync(path.join(dist, 'dmg-root/Plume.app/Contents/MacOS/plume'), 'utf8'), 'app');
  assert.deepEqual(calls.map(([cmd]) => cmd), ['hdiutil']);
  assert.ok(fs.readFileSync(path.join(dist, 'dmg-root/.DS_Store')).equals(fs.readFileSync('crates/plume-app/packaging/dmg-layout.ds-store')));
  assert.ok(calls.at(-1)[1].includes('UDZO'));
});

test('native payload verification uses platform installation tools', () => {
  const calls = [];
  for (const platform of ['win32', 'linux']) {
    extractInstaller({ platform, installer: '/tmp/installer', destination: '/tmp/test-install', run: (...args) => calls.push(args) });
  }
  assert.ok(calls[0][1].includes(`/DIR=${path.join('/tmp/test-install', 'Plume')}`));
  assert.equal(calls[1][0], 'dpkg-deb');
});
