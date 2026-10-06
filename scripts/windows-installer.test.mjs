import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { buildInstaller, extractInstaller } from './installer.mjs';

test('Windows installer installs, upgrades and uninstalls without removing user data', {
  skip: process.platform !== 'win32' || !process.env.PLUME_WINDOWS_TEST_BUILD,
}, t => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'plume-installer-test-'));
  const stage = path.join(root, 'stage');
  const dist = path.join(root, 'dist');
  const installed = path.join(root, 'installed');
  fs.mkdirSync(stage); fs.mkdirSync(dist); fs.mkdirSync(installed);
  const run = (cmd, args) => {
    const result = spawnSync(cmd, args, { encoding: 'utf8' });
    assert.equal(result.status, 0, `${cmd}: ${result.error?.message || result.stderr || result.stdout}`);
    return result.stdout;
  };
  t.after(() => {
    const uninstall = path.join(installed, 'Plume/unins000.exe');
    if (fs.existsSync(path.join(installed, 'Plume/plume.exe')) && fs.existsSync(uninstall)) run(uninstall, ['/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART']);
    // The uninstaller removes its own bootstrap executable after returning.
    fs.rmSync(root, { recursive: true, force: true, maxRetries: 20, retryDelay: 100 });
  });
  for (const file of fs.readdirSync(process.env.PLUME_WINDOWS_TEST_BUILD).filter(n => /^(plume|plume-updater)\.exe$|\.dll$/i.test(n))) {
    fs.copyFileSync(path.join(process.env.PLUME_WINDOWS_TEST_BUILD, file), path.join(stage, file));
  }
  const version = run(path.join(stage, 'plume.exe'), ['--version']).trim().replace('Plume ', '');
  // A separate application ID keeps the developer's installed Plume untouched.
  const installer = buildInstaller({ platform: 'win32', version, target: 'x86_64-pc-windows-msvc', stage, dist, run, appId: `Plume.PackagingTest.${process.pid}` });
  fs.writeFileSync(path.join(root, 'history.json'), 'user history');
  const app = extractInstaller({ platform: 'win32', installer, destination: installed, run });
  assert.equal(run(path.join(app, 'plume.exe'), ['--version']).trim(), `Plume ${version}`);
  assert.match(run(path.join(app, 'plume-updater.exe'), ['--help']), /Plume updater/);
  assert.ok(fs.existsSync(path.join(app, 'unins000.exe')));
  assert.ok(fs.readdirSync(app).every(file => !file.includes('shell')));
  fs.writeFileSync(path.join(stage, 'upgrade-marker.txt'), 'upgrade');
  fs.rmSync(installer);
  buildInstaller({ platform: 'win32', version, target: 'x86_64-pc-windows-msvc', stage, dist, run, appId: `Plume.PackagingTest.${process.pid}` });
  extractInstaller({ platform: 'win32', installer, destination: installed, run });
  assert.equal(fs.readFileSync(path.join(app, 'upgrade-marker.txt'), 'utf8'), 'upgrade');
  run(path.join(app, 'unins000.exe'), ['/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART']);
  assert.ok(!fs.existsSync(path.join(app, 'plume.exe')));
  assert.equal(fs.readFileSync(path.join(root, 'history.json'), 'utf8'), 'user history');
});
