import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const script = fileURLToPath(new URL('../release.mjs', import.meta.url));
function run(cwd, command, args, options = {}) {
  const r = spawnSync(command, args, { cwd, encoding: 'utf8', ...options });
  assert.ifError(r.error);
  return r;
}
function git(cwd, ...args) {
  const r = run(cwd, 'git', args);
  assert.equal(r.status, 0, r.stderr);
  return r.stdout.trim();
}
function fixture(t, inherited = false) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'plume-release-'));
  // Git/filesystem background work can briefly keep fixture directories busy.
  t.after(() => fs.rmSync(dir, { recursive: true, force: true, maxRetries: 5, retryDelay: 50 }));
  const repo = path.join(dir, 'repo');
  fs.mkdirSync(repo);
  git(repo, 'init', '-b', 'main');
  git(repo, 'config', 'user.name', 'Release Test');
  git(repo, 'config', 'user.email', 'release@example.invalid');
  git(repo, 'config', 'core.autocrlf', 'false');
  git(repo, 'config', 'commit.gpgsign', 'false');
  git(repo, 'config', 'tag.gpgsign', 'false');
  fs.writeFileSync(path.join(repo, 'Cargo.toml'), '[workspace]\nresolver = "2"\nmembers = ["crates/*"]\n' + (inherited ? '\n[workspace.package]\nversion = "1.2.3"\n' : ''));
  for (const name of ['one', 'two']) {
    fs.mkdirSync(path.join(repo, 'crates', name, 'src'), { recursive: true });
    fs.writeFileSync(path.join(repo, 'crates', name, 'src/lib.rs'), 'pub fn hello() {}\n');
    fs.writeFileSync(path.join(repo, 'crates', name, 'Cargo.toml'), `[package]\nname = "${name}"\n${inherited ? 'version.workspace = true' : 'version = "1.2.3"'}\nedition = "2021"\n`);
  }
  const generated = run(repo, 'cargo', ['generate-lockfile', '--offline']);
  assert.equal(generated.status, 0, generated.stderr);
  // Unused external entries are deliberately identical in version/name to local
  // entries, to verify surgical editing rather than global string replacement.
  fs.appendFileSync(path.join(repo, 'Cargo.lock'), '\n[[package]]\nname = "one"\nversion = "1.2.3"\nsource = "registry+https://github.com/rust-lang/crates.io-index"\nchecksum = "' + 'a'.repeat(64) + '"\n');
  git(repo, 'add', '.');
  git(repo, 'commit', '-m', 'Initial');
  return { dir, repo, release: (...args) => run(repo, process.execPath, [script, ...args]), read: f => fs.readFileSync(path.join(repo, f), 'utf8') };
}
function success(r) { assert.equal(r.status, 0, r.stderr + r.stdout); }
function rejected(r, pattern) { assert.notEqual(r.status, 0); assert.match(r.stderr, pattern); }

for (const [bump, expected] of [['patch', '1.2.4'], ['minor', '1.3.0'], ['major', '2.0.0']]) {
  test(`${bump}: common SemVer, annotated tag and untouched external lock entry`, t => {
    const f = fixture(t);
    const markdown = '# Changes\n\n- **Bold** & `code`\n\n```rust\nhello();\n```\n';
    success(f.release(bump, '--notes', markdown));
    assert.equal(f.read(`releases/v${expected}.md`), markdown);
    assert.match(f.read('crates/one/Cargo.toml'), new RegExp(`version = "${expected}"`));
    assert.match(f.read('crates/two/Cargo.toml'), new RegExp(`version = "${expected}"`));
    assert.match(f.read('Cargo.lock'), /name = "one"\nversion = "1\.2\.3"\nsource = "registry/);
    assert.equal(git(f.repo, 'cat-file', '-t', `v${expected}`), 'tag');
    assert.equal(git(f.repo, 'status', '--porcelain'), '');
    success(f.release('--check-tag', `v${expected}`));
    rejected(f.release('tag'), /already exists/);
    rejected(f.release('--check-tag', 'v9.0.0'), /mismatch/);
  });
}

test('dry-run leaves files, index, refs and repository directories untouched', t => {
  const f = fixture(t);
  const lock = f.read('Cargo.lock');
  const head = git(f.repo, 'rev-parse', 'HEAD');
  const index = fs.readFileSync(path.join(f.repo, '.git/index'));
  success(f.release('patch', '--notes', '# Dry run', '--dry-run', '--push'));
  assert.equal(f.read('Cargo.lock'), lock);
  assert.deepEqual(fs.readFileSync(path.join(f.repo, '.git/index')), index);
  assert.equal(git(f.repo, 'rev-parse', 'HEAD'), head);
  assert.equal(git(f.repo, 'tag'), '');
  assert.equal(fs.existsSync(path.join(f.repo, 'releases')), false);
});

test('PR preparation on branch, inherited workspace version, then tag merged main', t => {
  const f = fixture(t, true);
  git(f.repo, 'switch', '-c', 'release-pr');
  rejected(f.release('patch', '--notes', '# Changes'), /main/);
  const notes = '# PR release\n\nUnicode: été 🦀\n';
  // Notes file is outside the checkout, so it does not dirty the worktree.
  const notesFile = path.join(f.dir, 'notes.md');
  fs.writeFileSync(notesFile, notes);
  success(f.release('minor', '--notes-file', notesFile, '--prepare-only'));
  assert.match(f.read('Cargo.toml'), /version = "1.3.0"/);
  assert.match(f.read('crates/one/Cargo.toml'), /version.workspace = true/);
  assert.equal(git(f.repo, 'tag'), '');
  git(f.repo, 'add', '.');
  git(f.repo, 'commit', '-m', 'Prepare release');
  git(f.repo, 'switch', 'main');
  git(f.repo, 'merge', '--no-ff', 'release-pr', '-m', 'Merge release PR');
  const head = git(f.repo, 'rev-parse', 'HEAD');
  success(f.release('tag'));
  assert.equal(git(f.repo, 'rev-parse', 'HEAD'), head);
  assert.equal(f.read('releases/v1.3.0.md'), notes);
  success(f.release('--check-tag', 'v1.3.0'));
});

test('stdin Markdown is preserved verbatim', t => {
  const f = fixture(t);
  const notes = '# Stdin\r\n\r\n- A\r\n';
  const r = run(f.repo, process.execPath, [script, 'patch', '--notes-file', '-', '--prepare-only'], { input: notes });
  success(r);
  assert.equal(f.read('releases/v1.2.4.md'), notes);
});

test('root package and inherited members both increment their own sections', t => {
  const f = fixture(t, true);
  fs.mkdirSync(path.join(f.repo, 'src'));
  fs.writeFileSync(path.join(f.repo, 'src/lib.rs'), 'pub fn root() {}\n');
  fs.appendFileSync(path.join(f.repo, 'Cargo.toml'), '\n[package]\nname = "zzz-root"\nversion = "1.2.3"\nedition = "2021"\n');
  // Put the inheritance section before the root package, so an unscoped text
  // replacement would modify the wrong occurrence of the version.
  const generated = run(f.repo, 'cargo', ['generate-lockfile', '--offline']);
  success(generated);
  git(f.repo, 'add', '.');
  git(f.repo, 'commit', '-m', 'Add root package');
  success(f.release('patch', '--notes', '# Root'));
  assert.equal([...f.read('Cargo.toml').matchAll(/version = "1.2.4"/g)].length, 2);
  success(f.release('--check-tag', 'v1.2.4'));
});

test('version-qualified workspace lock references increment without changing external references', t => {
  const f = fixture(t);
  const lock = f.read('Cargo.lock').replace('name = "two"\nversion = "1.2.3"', 'name = "two"\nversion = "1.2.3"\ndependencies = [\n "one 1.2.3",\n "one 1.2.3 (registry+https://github.com/rust-lang/crates.io-index)",\n]');
  fs.writeFileSync(path.join(f.repo, 'Cargo.lock'), lock);
  git(f.repo, 'add', '.');
  git(f.repo, 'commit', '-m', 'Qualified lock references');
  success(f.release('major', '--notes', '# Lock'));
  assert.match(f.read('Cargo.lock'), /"one 2.0.0"/);
  assert.match(f.read('Cargo.lock'), /"one 1.2.3 \(registry\+/);
});

test('check-tag rejects lightweight tags and missing or changed notes', t => {
  const f = fixture(t);
  git(f.repo, 'tag', 'v1.2.3');
  rejected(f.release('--check-tag', 'v1.2.3'), /annotated/);
  git(f.repo, 'tag', '-d', 'v1.2.3');
  git(f.repo, 'tag', '-a', 'v1.2.3', '-m', 'Missing notes');
  rejected(f.release('--check-tag', 'v1.2.3'), /did not match|pathspec/);
  git(f.repo, 'tag', '-d', 'v1.2.3');
  success(f.release('patch', '--notes', '# Valid'));
  fs.writeFileSync(path.join(f.repo, 'releases/v1.2.4.md'), '# Changed');
  rejected(f.release('--check-tag', 'v1.2.4'), /differ/);
  rejected(f.release('--check-tag', 'v01.2.4'), /Expected tag/);
});

test('reject dirty tracked, staged or untracked files', t => {
  const f = fixture(t);
  fs.writeFileSync(path.join(f.repo, 'untracked'), 'dirty');
  rejected(f.release('patch', '--notes', '# Changes'), /dirty/);
  git(f.repo, 'add', '.');
  rejected(f.release('patch', '--notes', '# Changes'), /dirty/);
  git(f.repo, 'commit', '-m', 'file');
  fs.appendFileSync(path.join(f.repo, 'Cargo.toml'), '# dirty\n');
  rejected(f.release('patch', '--notes', '# Changes', '--dry-run'), /dirty/);
});

test('reject inconsistent lockfile and manifest versions', t => {
  const f = fixture(t);
  fs.writeFileSync(path.join(f.repo, 'Cargo.lock'), f.read('Cargo.lock').replace('version = "1.2.3"', 'version = "1.2.2"'));
  git(f.repo, 'add', '.');
  git(f.repo, 'commit', '-m', 'Bad lock');
  rejected(f.release('patch', '--notes', '# Changes'), /Cargo.lock|lock file/);
  fs.writeFileSync(path.join(f.repo, 'crates/two/Cargo.toml'), f.read('crates/two/Cargo.toml').replace('1.2.3', '1.3.0'));
  git(f.repo, 'add', '.');
  git(f.repo, 'commit', '-m', 'Bad manifest');
  rejected(f.release('patch', '--notes', '# Changes'), /Inconsistent|lock file/);
});

test('failed commit restores original bytes and index, removes new notes', t => {
  const f = fixture(t);
  const files = ['Cargo.toml', 'crates/one/Cargo.toml', 'crates/two/Cargo.toml', 'Cargo.lock'];
  const original = files.map(f.read);
  const index = fs.readFileSync(path.join(f.repo, '.git/index'));
  const head = git(f.repo, 'rev-parse', 'HEAD');
  const hook = path.join(f.repo, '.git/hooks/pre-commit');
  fs.writeFileSync(hook, '#!/bin/sh\necho "test commit failure" >&2\nexit 1\n', { mode: 0o755 });
  rejected(f.release('patch', '--notes', '# Changes'), /test commit failure/);
  assert.deepEqual(files.map(f.read), original);
  assert.deepEqual(fs.readFileSync(path.join(f.repo, '.git/index')), index);
  assert.equal(git(f.repo, 'status', '--porcelain'), '');
  assert.equal(git(f.repo, 'rev-parse', 'HEAD'), head);
  assert.equal(git(f.repo, 'tag'), '');
  assert.equal(fs.existsSync(path.join(f.repo, 'releases')), false);
  assert.ok(index.length);
});

function remote(f) {
  const bare = path.join(f.dir, 'origin.git');
  git(f.dir, 'init', '--bare', bare);
  git(f.repo, 'remote', 'add', 'origin', bare);
  git(f.repo, 'push', '-u', 'origin', 'main');
  return bare;
}
test('successful atomic push sends main and annotated tag together', t => {
  const f = fixture(t);
  const bare = remote(f);
  success(f.release('patch', '--notes', '# Atomic', '--push'));
  assert.equal(git(bare, 'rev-parse', 'main'), git(f.repo, 'rev-parse', 'HEAD'));
  assert.equal(git(bare, 'rev-parse', 'v1.2.4^{commit}'), git(bare, 'rev-parse', 'main'));
});

test('server refusal rolls back neither local commit nor tag, remote is atomic', t => {
  const f = fixture(t);
  const bare = remote(f);
  const before = git(bare, 'rev-parse', 'main');
  fs.writeFileSync(path.join(bare, 'hooks/update'), '#!/bin/sh\ncase "$1" in refs/tags/*) echo "reject tag" >&2; exit 1;; esac\n', { mode: 0o755 });
  rejected(f.release('patch', '--notes', '# Rejected push', '--push'), /atomic|reject tag/);
  assert.equal(git(bare, 'rev-parse', 'main'), before);
  assert.equal(git(bare, 'tag'), '');
  assert.notEqual(git(f.repo, 'rev-parse', 'HEAD'), before);
  assert.equal(git(f.repo, 'tag'), 'v1.2.4');
  assert.equal(git(f.repo, 'status', '--porcelain'), '');
});

test('--push refuses HEAD ahead of origin/main before modifying files', t => {
  const f = fixture(t);
  remote(f);
  git(f.repo, 'commit', '--allow-empty', '-m', 'Ahead');
  const lock = f.read('Cargo.lock');
  rejected(f.release('patch', '--notes', '# Ahead', '--push'), /HEAD must equal/);
  assert.equal(f.read('Cargo.lock'), lock);
  assert.equal(git(f.repo, 'tag'), '');
});

test('tag --push on merged main and remote tag collision rejection', t => {
  const f = fixture(t);
  const bare = remote(f);
  success(f.release('patch', '--notes', '# PR', '--prepare-only'));
  git(f.repo, 'add', '.');
  git(f.repo, 'commit', '-m', 'Merged PR');
  git(f.repo, 'push', 'origin', 'main');
  success(f.release('tag', '--push'));
  assert.equal(git(bare, 'rev-parse', 'v1.2.4^{commit}'), git(bare, 'rev-parse', 'main'));
  git(f.repo, 'tag', '-d', 'v1.2.4');
  rejected(f.release('tag', '--push'), /Remote tag already exists/);
});

test('empty notes, unknown options and missing notes are refused', t => {
  const f = fixture(t);
  rejected(f.release('patch', '--notes', ' \n'), /non-empty/);
  rejected(f.release('patch'), /Provide/);
  rejected(f.release('patch', '--unknown'), /Unknown/);
  rejected(f.release('patch', '--notes', '--dry-run'), /Missing value/);
  rejected(f.release('patch', '--notes', '# A', '--notes-file', '-'), /one notes source/);
  rejected(f.release('patch', '--prepare-only', '--push'), /cannot be combined/);
});
