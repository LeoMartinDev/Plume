#!/usr/bin/env node
// Node 22+, Git and Cargo; no package installation required.
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const semver = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;
function fail(message) { throw new Error(message); }
function run(command, args, options = {}) {
  const r = spawnSync(command, args, { encoding: 'utf8', ...options });
  if (r.error || r.status !== 0) fail(`${command} ${args.join(' ')}: ${r.error?.message ?? r.stderr ?? r.stdout}`);
  return r.stdout.trim();
}
const git = (...args) => run('git', ['--no-optional-locks', ...args]);
function field(section, key) {
  const matches = [...section.matchAll(new RegExp(`^${key}\\s*=\\s*"([^"]+)"\\s*(?:#.*)?$`, 'gm'))];
  if (matches.length !== 1) fail(`Expected exactly one literal ${key}`);
  return matches[0][1];
}
function sectionMatch(text, name) {
  const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const match = text.match(new RegExp(`^(\\[${escaped}\\][ \\t]*(?:#.*)?\\r?\\n)([\\s\\S]*?)(?=^\\[|$(?![\\s\\S]))`, 'm'));
  if (!match) fail(`Missing [${name}]`);
  return match;
}
function section(text, name) { return sectionMatch(text, name)[2]; }
function editSection(text, name, edit) {
  const match = sectionMatch(text, name);
  return text.slice(0, match.index) + match[1] + edit(match[2]) + text.slice(match.index + match[0].length);
}
function replaceVersion(text, oldVersion, nextVersion) {
  return text.replace(/^(version\s*=\s*")([^"]+)(")/m, (_, a, v, b) => {
    if (v !== oldVersion) fail(`Inconsistent version ${v}, expected ${oldVersion}`);
    return a + nextVersion + b;
  });
}

export function workspace() {
  // Cargo resolves members, including inherited versions and glob members. --locked
  // and --offline prohibit dependency resolution updates; no build is performed.
  const metadata = JSON.parse(run('cargo', ['metadata', '--no-deps', '--format-version', '1', '--offline', '--locked']));
  const root = fs.realpathSync(metadata.workspace_root);
  if (fs.realpathSync(process.cwd()) !== root) fail('Run release.mjs at the workspace root');
  const crates = metadata.packages.filter(p => metadata.workspace_members.includes(p.id));
  if (!crates.length) fail('Empty workspace');
  const version = crates[0].version;
  if (!semver.test(version) || crates.some(p => p.version !== version)) fail('Inconsistent workspace versions (expected common MAJOR.MINOR.PATCH)');
  const manifests = new Map();
  const manifestSections = new Map();
  function addManifest(filename, content, targetSection) {
    manifests.set(filename, content);
    if (!manifestSections.has(filename)) manifestSections.set(filename, new Set());
    manifestSections.get(filename).add(targetSection);
  }
  for (const crate of crates) {
    const filename = path.relative(root, crate.manifest_path);
    if (filename.startsWith('..') || path.isAbsolute(filename)) fail('Workspace member outside repository');
    const content = fs.readFileSync(filename, 'utf8');
    const pkg = section(content, 'package');
    if (/^version\.workspace\s*=\s*true\s*(?:#.*)?$/m.test(pkg)) {
      const rootContent = fs.readFileSync('Cargo.toml', 'utf8');
      if (field(section(rootContent, 'workspace.package'), 'version') !== version) fail('Inconsistent inherited version');
      addManifest('Cargo.toml', rootContent, 'workspace.package');
    } else {
      if (field(pkg, 'version') !== version) fail('Inconsistent manifest version');
      addManifest(filename, content, 'package');
    }
  }
  const lock = fs.readFileSync('Cargo.lock', 'utf8');
  const names = new Set(crates.map(p => p.name));
  const found = new Set();
  const blocks = lock.split(/(?=^\[\[package\]\])/m);
  for (const block of blocks.slice(1)) {
    const name = field(block, 'name');
    // Registry/git packages with the same name are external and untouched.
    if (!names.has(name) || /^source\s*=/m.test(block)) continue;
    if (found.has(name) || field(block, 'version') !== version) fail(`Inconsistent Cargo.lock entry for ${name}`);
    found.add(name);
  }
  if (found.size !== names.size) fail('Missing workspace package in Cargo.lock');
  return { version, manifests, manifestSections, lock, blocks, names };
}

function absentTag(tag) {
  if (spawnSync('git', ['show-ref', '--verify', '--quiet', `refs/tags/${tag}`]).status === 0) fail(`Tag already exists: ${tag}`);
}
function originHead() {
  git('fetch', 'origin', '+refs/heads/main:refs/remotes/origin/main');
  return git('rev-parse', 'refs/remotes/origin/main');
}
function remoteTagAbsent(tag) {
  if (git('ls-remote', '--tags', 'origin', `refs/tags/${tag}`).length) fail(`Remote tag already exists: ${tag}`);
}
function notesValid(notes) { if (!notes.trim()) fail('Release notes must contain non-empty Markdown'); }

export function main(args = process.argv.slice(2)) {
  if (Number(process.versions.node.split('.')[0]) < 22) fail('Node.js 22+ is required');
  const command = args.shift();
  if (command === '--help' || !command) {
    console.log('Usage: node release.mjs patch|minor|major --notes-file FILE|- | --notes TEXT [--dry-run] [--prepare-only] [--push]\n       node release.mjs tag [--dry-run] [--push]\n       node release.mjs --check-tag vMAJOR.MINOR.PATCH');
    return;
  }
  const opts = {};
  for (let i = 0; i < args.length; i++) {
    const key = args[i];
    if (!['--notes-file', '--notes', '--dry-run', '--prepare-only', '--push'].includes(key) || key in opts) fail(`Unknown or duplicate option: ${key}`);
    opts[key] = ['--notes-file', '--notes'].includes(key) ? args[++i] : true;
    if (opts[key] === undefined || (typeof opts[key] === 'string' && opts[key].startsWith('--'))) fail(`Missing value for ${key}`);
  }
  if (!['patch', 'minor', 'major', 'tag'].includes(command)) fail(`Unknown command: ${command}`);
  if (opts['--prepare-only'] && (opts['--push'] || command === 'tag')) fail('--prepare-only cannot be combined with tag or --push');
  if (opts['--notes'] !== undefined && opts['--notes-file'] !== undefined) fail('Choose one notes source');
  if (command === 'tag' && (opts['--notes'] !== undefined || opts['--notes-file'] !== undefined)) fail('tag uses the committed release notes');
  if (git('status', '--porcelain', '--untracked-files=all')) fail('Checkout is dirty');
  if (git('rev-parse', '--show-prefix')) fail('Run at repository root');
  if (!opts['--prepare-only'] && git('branch', '--show-current') !== 'main') fail('Tags can only be created on main');
  const state = workspace();
  const parts = state.version.split('.').map(BigInt);
  if (command !== 'tag') {
    const idx = ['major', 'minor', 'patch'].indexOf(command);
    parts[idx]++;
    for (let i = idx + 1; i < 3; i++) parts[i] = 0n;
  }
  const version = parts.join('.');
  const tag = `v${version}`;
  absentTag(tag);
  const notesPath = `releases/${tag}.md`;
  let notes;
  if (command === 'tag') {
    git('ls-files', '--error-unmatch', notesPath);
    notes = fs.readFileSync(notesPath, 'utf8');
  } else {
    if (fs.existsSync(notesPath)) fail(`Notes already exist: ${notesPath}`);
    if (opts['--notes-file'] !== undefined) notes = fs.readFileSync(opts['--notes-file'] === '-' ? 0 : opts['--notes-file'], 'utf8');
    else if (opts['--notes'] !== undefined) notes = opts['--notes'];
    else fail('Provide --notes-file FILE|- or --notes TEXT');
  }
  notesValid(notes);
  const changes = new Map();
  if (command !== 'tag') {
    for (const [filename, content] of state.manifests) {
      let next = content;
      for (const targetSection of state.manifestSections.get(filename)) {
        next = editSection(next, targetSection, old => replaceVersion(old, state.version, version));
      }
      changes.set(filename, next);
    }
    changes.set('Cargo.lock', state.blocks.map((block, i) => {
      let next = i && state.names.has(field(block, 'name')) && !/^source\s*=/m.test(block) ? replaceVersion(block, state.version, version) : block;
      // Cargo disambiguates packages of the same name with version-qualified
      // references. A registry/git reference additionally includes its source,
      // and must remain byte-for-byte unchanged.
      next = next.replace(/^(\s*")([^" ]+) ([^" ]+)(",?\s*)$/gm, (line, a, name, oldVersion, b) => state.names.has(name) && oldVersion === state.version ? `${a}${name} ${version}${b}` : line);
      return next;
    }).join(''));
    changes.set(notesPath, notes);
  }
  console.log(`${command}: ${state.version} -> ${version}\n${[...changes.keys()].join('\n')}\n${opts['--prepare-only'] ? 'Prepare files for PR' : `Commit (if bump) and annotated tag ${tag}`}${opts['--push'] ? '; atomic push main + tag to origin' : ''}`);
  if (opts['--dry-run']) return;
  const originalHead = git('rev-parse', 'HEAD');
  if (opts['--push']) {
    if (originHead() !== originalHead) fail('Before --push, HEAD must equal origin/main');
    remoteTagAbsent(tag);
  }
  const indexPath = git('rev-parse', '--git-path', 'index');
  const index = fs.existsSync(indexPath) ? fs.readFileSync(indexPath) : null;
  const originals = new Map([...changes.keys()].map(f => [f, fs.existsSync(f) ? fs.readFileSync(f) : null]));
  const releasesExisted = fs.existsSync('releases');
  try {
    for (const [filename, content] of changes) {
      fs.mkdirSync(path.dirname(filename), { recursive: true });
      fs.writeFileSync(filename, content);
    }
    if (opts['--prepare-only']) return;
    if (command !== 'tag') {
      git('add', '--', ...changes.keys());
      git('commit', '-m', `Release ${tag}`);
    }
  } catch (error) {
    for (const [filename, content] of originals) {
      if (content === null) fs.rmSync(filename, { force: true });
      else fs.writeFileSync(filename, content);
    }
    if (!releasesExisted && fs.existsSync('releases') && !fs.readdirSync('releases').length) fs.rmdirSync('releases');
    if (index === null) fs.rmSync(indexPath, { force: true });
    else fs.writeFileSync(indexPath, index);
    throw error;
  }
  git('-c', 'tag.gpgsign=false', 'tag', '-a', tag, '-F', notesPath);
  if (opts['--push']) {
    // Detect a concurrent update before pushing; the server also rejects a
    // non-fast-forward or protected main. Never force, never roll back locally.
    if (originHead() !== originalHead) fail('origin/main changed; local release commit/tag retained');
    remoteTagAbsent(tag);
    git('push', '--atomic', 'origin', 'HEAD:refs/heads/main', `refs/tags/${tag}:refs/tags/${tag}`);
  }
}

export function checkTag(tag) {
  if (!tag?.startsWith('v') || !semver.test(tag.slice(1))) fail('Expected tag vMAJOR.MINOR.PATCH');
  const { version } = workspace();
  if (tag !== `v${version}`) fail(`Tag/version mismatch: ${tag} / ${version}`);
  if (git('rev-parse', `${tag}^{commit}`) !== git('rev-parse', 'HEAD')) fail('Tag does not point to HEAD');
  if (git('cat-file', '-t', `refs/tags/${tag}`) !== 'tag') fail('Release tag must be annotated');
  const filename = `releases/${tag}.md`;
  git('ls-files', '--error-unmatch', filename);
  const committed = git('show', `HEAD:${filename}`);
  notesValid(committed);
  if (fs.readFileSync(filename, 'utf8').trim() !== committed) fail('Notes differ from committed notes');
  if (git('status', '--porcelain', '--untracked-files=all')) fail('Checkout is dirty');
  console.log(`Verified ${tag}: workspace, lockfile, annotated tag and Markdown notes`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    if (process.argv[2] === '--check-tag') {
      if (process.argv.length !== 4) fail('Usage: --check-tag TAG');
      checkTag(process.argv[3]);
    } else main();
  } catch (error) { console.error(error.message); process.exitCode = 1; }
}
