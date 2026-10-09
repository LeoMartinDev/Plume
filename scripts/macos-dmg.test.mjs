import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { buildInstaller, extractInstaller, targets } from './installer.mjs';

test('real macOS DMG preserves the Finder layout, Applications shortcut and copied app', {
  skip: process.platform !== 'darwin' || !process.env.PLUME_DMG_TEST,
}, t => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'plume-dmg-native-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const stage = path.join(root, 'stage');
  const dist = path.join(root, 'dist');
  const app = path.join(stage, 'Plume.app');
  fs.mkdirSync(path.join(app, 'Contents/MacOS'), { recursive: true });
  fs.mkdirSync(path.join(app, 'Contents/Resources'));
  fs.mkdirSync(dist);
  fs.copyFileSync('crates/plume-app/packaging/Info.plist', path.join(app, 'Contents/Info.plist'));
  fs.copyFileSync('crates/plume-app/assets/brand/plume.icns', path.join(app, 'Contents/Resources/plume.icns'));
  const run = (cmd, args) => {
    const result = spawnSync(cmd, args, { encoding: 'utf8' });
    assert.equal(result.status, 0, `${cmd}: ${result.error?.message || result.stderr || result.stdout || result.signal}`);
    return result.stdout;
  };
  // Re-signing a copied Apple binary can be killed by macOS's signature cache.
  // Build our own executable so the fixture tests the same signing path as Plume.
  const source = path.join(root, 'fixture.c');
  fs.writeFileSync(source, 'int main(void) { return 0; }\n');
  run('clang', [source, '-o', path.join(app, 'Contents/MacOS/plume')]);
  run('codesign', ['--force', '--sign', '-', app]);
  const installer = buildInstaller({ platform: 'darwin', version: '1.2.3', target: targets[2], stage, dist, run });
  run('hdiutil', ['verify', installer]);
  const installed = extractInstaller({ platform: 'darwin', installer, destination: path.join(root, 'installed'), run });
  assert.ok(fs.readFileSync(path.join(installed, 'plume')).equals(fs.readFileSync(path.join(app, 'Contents/MacOS/plume'))));
  run('codesign', ['--verify', '--deep', '--strict', path.resolve(installed, '../..')]);
  run(path.join(installed, 'plume'), []);
  assert.ok(!fs.existsSync(path.join(root, 'installed/dmg-mount/Plume.app')));
});
