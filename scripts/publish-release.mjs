// GitHub REST API using only Node built-ins. Called after every validation and
// native archive job succeeds. Never converts or overwrites a published release.
import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { checkTag } from '../release.mjs';

export async function publish({ repository, token, tag, sha, directory, notes, request = fetch }) {
  if (!repository || !token || !/^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(tag) || !sha) throw new Error('Missing/invalid GitHub release context');
  const targets = ['x86_64-unknown-linux-gnu', 'x86_64-pc-windows-msvc', 'x86_64-apple-darwin', 'aarch64-apple-darwin'];
  const expected = targets.flatMap(target => {
    const file = `stt-${tag}-${target}.${target.includes('windows') ? 'zip' : 'tar.gz'}`;
    return [file, `${file}.sha256`];
  });
  const filenames = fs.readdirSync(directory).sort();
  if (JSON.stringify(filenames) !== JSON.stringify(expected.sort())) throw new Error('Expected exactly four native archives and their checksums');
  for (const filename of filenames.filter(f => !f.endsWith('.sha256'))) {
    const digest = createHash('sha256').update(fs.readFileSync(path.join(directory, filename))).digest('hex');
    if (fs.readFileSync(path.join(directory, filename + '.sha256'), 'utf8') !== `${digest}  ${filename}\n`) throw new Error(`Checksum mismatch: ${filename}`);
  }
  const base = `https://api.github.com/repos/${repository}`;
  async function api(url, method = 'GET', body, binary = false) {
    const response = await request(url.startsWith('https://') ? url : base + url, {
      method,
      headers: { Authorization: `Bearer ${token}`, Accept: 'application/vnd.github+json', 'X-GitHub-Api-Version': '2022-11-28', 'Content-Type': binary ? 'application/octet-stream' : 'application/json' },
      body: body === undefined ? undefined : binary ? body : JSON.stringify(body),
    });
    if (!response.ok) throw new Error(`GitHub ${method} failed (${response.status}): ${await response.text()}`);
    return response.status === 204 ? null : response.json();
  }
  let release;
  for (let page = 1; ; page++) {
    const releases = await api(`/releases?per_page=100&page=${page}`);
    release = releases.find(r => r.tag_name === tag);
    if (release || releases.length < 100) break;
  }
  async function guard() {
    const current = await api(`/releases/${release.id}`);
    if (!current.draft || current.tag_name !== tag) throw new Error('Refusing to overwrite a published or changed release');
    return current;
  }
  const body = notes + '\n\n---\nThese archives are not publisher-signed or notarized. macOS uses local ad-hoc signatures only.\n';
  if (release) {
    await guard();
    release = await api(`/releases/${release.id}`, 'PATCH', { name: tag, body, target_commitish: sha });
  } else {
    release = await api('/releases', 'POST', { tag_name: tag, target_commitish: sha, name: tag, body, draft: true });
  }
  const current = await guard();
  // Remove stale assets from an earlier attempt only while still a draft.
  for (const asset of current.assets) {
    await guard();
    await api(`/releases/assets/${asset.id}`, 'DELETE');
  }
  for (const filename of filenames) {
    await guard();
    const upload = release.upload_url.replace(/\{.*$/, '') + `?name=${encodeURIComponent(filename)}`;
    await api(upload, 'POST', fs.readFileSync(path.join(directory, filename)), true);
  }
  console.log(`Draft ready: ${release.html_url}`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const tag = process.env.GITHUB_REF_NAME;
    checkTag(tag);
    await publish({ repository: process.env.GITHUB_REPOSITORY, token: process.env.GITHUB_TOKEN, tag, sha: process.env.GITHUB_SHA, directory: 'release-assets', notes: fs.readFileSync(`releases/${tag}.md`, 'utf8') });
  } catch (error) { console.error(error.message); process.exitCode = 1; }
}
