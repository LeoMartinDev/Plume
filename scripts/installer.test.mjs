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
    'Plume-v1.2.3-aarch64-apple-darwin.pkg',
  ]);
  assert.throws(() => installerName('1.2.3', 'unsupported'));
});

test('macOS package includes only one app bundle and installs in Applications', t => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'plume-pkg-test-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const stage = path.join(root, 'stage');
  const dist = path.join(root, 'dist');
  fs.mkdirSync(path.join(stage, 'Plume.app/Contents/MacOS'), { recursive: true });
  fs.mkdirSync(dist);
  fs.writeFileSync(path.join(stage, 'Plume.app/Contents/MacOS/plume'), 'app');
  fs.writeFileSync(path.join(stage, 'plume'), 'duplicate outside bundle');
  const calls = [];
  buildInstaller({ platform: 'darwin', version: '1.2.3', target: targets[2], stage, dist, run: (...args) => calls.push(args) });
  assert.deepEqual(fs.readdirSync(path.join(dist, 'pkg-root')), ['Applications']);
  assert.equal(fs.readFileSync(path.join(dist, 'pkg-root/Applications/Plume.app/Contents/MacOS/plume'), 'utf8'), 'app');
  const build = calls.find(([cmd, args]) => cmd === 'pkgbuild' && args.includes('--identifier'));
  assert.equal(build[1][build[1].indexOf('--install-location') + 1], '/');
  assert.ok(calls.some(([cmd, args]) => cmd.endsWith('PlistBuddy') && args.includes('Set :0:BundleIsRelocatable false')));
});

test('native payload verification uses installation tools rather than ZIP extraction', () => {
  const calls = [];
  for (const platform of ['win32', 'darwin', 'linux']) {
    extractInstaller({ platform, installer: '/tmp/installer', destination: '/tmp/test-install', run: (...args) => calls.push(args) });
  }
  assert.ok(calls[0][1].includes(`/DIR=${path.join('/tmp/test-install', 'Plume')}`));
  assert.equal(calls[1][0], 'pkgutil');
  assert.equal(calls[2][0], 'dpkg-deb');
});
