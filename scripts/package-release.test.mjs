import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
import { workspace } from '../release.mjs';

// Optional local integration check with existing native binaries. The release
// installer jobs always perform the same smoke test with fresh release builds.
test('native installer with real binaries, bundled runtimes and clean-environment startup', { skip: !process.env.PLUME_PACKAGE_TEST_BUILD }, t => {
  const { version } = workspace();
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'plume-native-package-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  fs.mkdirSync(path.join(root, 'src'));
  fs.writeFileSync(path.join(root, 'src/main.rs'), 'fn main() {}\n');
  fs.writeFileSync(path.join(root, 'Cargo.toml'), `[package]\nname = "native-package-test"\nversion = "${version}"\nedition = "2021"\n`);
  const lock = spawnSync('cargo', ['generate-lockfile', '--offline'], { cwd: root, encoding: 'utf8' });
  assert.equal(lock.status, 0, lock.stderr);
  const build = path.join(root, 'target/release');
  fs.mkdirSync(build, { recursive: true });
  for (const name of fs.readdirSync(process.env.PLUME_PACKAGE_TEST_BUILD).filter(n => /^(plume|plume-updater)(\.exe)?$/.test(n) || /\.(dll|dylib|so)(\.\d+)*$/i.test(n))) {
    fs.copyFileSync(path.join(process.env.PLUME_PACKAGE_TEST_BUILD, name), path.join(build, name));
    if (process.platform !== 'win32') fs.chmodSync(path.join(build, name), 0o755);
  }
  fs.writeFileSync(path.join(root, 'README.md'), '# Native package test\n');
  const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
  fs.cpSync(path.join(repo, 'crates/plume-app/packaging'), path.join(root, 'crates/plume-app/packaging'), { recursive: true });
  fs.cpSync(path.join(repo, 'crates/plume-app/assets/brand'), path.join(root, 'crates/plume-app/assets/brand'), { recursive: true });
  fs.mkdirSync(path.join(root, 'releases'));
  fs.writeFileSync(path.join(root, `releases/v${version}.md`), '# Fixture notes\n');
  const script = fileURLToPath(new URL('./package-release.mjs', import.meta.url));
  const result = spawnSync(process.execPath, [script], { cwd: root, encoding: 'utf8', env: { ...process.env, CARGO_TARGET_DIR: path.join(root, 'target') } });
  assert.equal(result.status, 0, result.stderr + result.stdout);
  assert.match(result.stdout, /Installer verified/);
  const dist = path.join(root, 'dist');
  const archive = fs.readdirSync(dist).find(n => /\.(exe|pkg|deb)$/.test(n));
  assert.ok(archive);
  const digest = createHash('sha256').update(fs.readFileSync(path.join(dist, archive))).digest('hex');
  assert.equal(fs.readFileSync(path.join(dist, archive + '.sha256'), 'utf8'), `${digest}  ${archive}\n`);
  if (process.platform === 'win32') {
    const stage = path.join(dist, `Plume-v${version}-x86_64-pc-windows-msvc`);
    const names = fs.readdirSync(stage).map(n => n.toLowerCase());
    for (const runtime of ['directml.dll', 'vulkan-1.dll', 'msvcp140.dll', 'vcruntime140.dll']) assert.ok(names.includes(runtime), `Missing ${runtime}`);
  }
});
