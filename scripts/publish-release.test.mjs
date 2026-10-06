import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { publish } from './publish-release.mjs';

function fixture(t) {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'stt-publish-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  for (const target of ['x86_64-unknown-linux-gnu', 'x86_64-pc-windows-msvc', 'aarch64-apple-darwin']) {
    const filename = `stt-v1.2.3-${target}.${target.includes('windows') ? 'zip' : 'tar.gz'}`;
    const content = Buffer.from('archive fixture ' + target);
    fs.writeFileSync(path.join(directory, filename), content);
    fs.writeFileSync(path.join(directory, filename + '.sha256'), `${createHash('sha256').update(content).digest('hex')}  ${filename}\n`);
  }
  return { repository: 'example/stt', token: 'test', tag: 'v1.2.3', sha: '123abc', directory, notes: '# Notes\n\n**Markdown**\n' };
}
function mock(existing) {
  const calls = [];
  const release = { id: 7, draft: true, tag_name: 'v1.2.3', upload_url: 'https://uploads.github.com/releases/7/assets{?name,label}', html_url: 'https://github.com/example/stt/releases/7', assets: [], ...existing };
  const request = async (url, options) => {
    calls.push({ url, ...options });
    let value = release;
    if (url.includes('/git/ref/tags/')) value = { object: { type: 'tag', sha: 'tagabc' } };
    if (url.includes('/git/tags/')) value = { object: { type: 'commit', sha: '123abc' } };
    if (url.includes('per_page=')) value = existing ? [release] : [];
    if (url.includes('uploads.github.com')) value = { id: 10 };
    return { ok: true, status: options.method === 'DELETE' ? 204 : 200, json: async () => value };
  };
  return { request, calls, release };
}

test('create draft with notes and all six verified assets', async t => {
  const f = fixture(t);
  const m = mock();
  await publish({ ...f, request: m.request });
  const created = m.calls.find(c => c.method === 'POST' && c.url.endsWith('/releases'));
  assert.equal(JSON.parse(created.body).draft, true);
  assert.equal(JSON.parse(created.body).target_commitish, undefined);
  assert.ok(JSON.parse(created.body).body.startsWith(f.notes));
  assert.equal(m.calls.filter(c => c.url.includes('uploads.github.com')).length, 6);
});

test('refresh draft and remove previous assets without publishing', async t => {
  const f = fixture(t);
  const m = mock({ assets: [{ id: 15 }] });
  await publish({ ...f, request: m.request });
  assert.equal(m.calls.filter(c => c.method === 'PATCH').length, 1);
  assert.equal(m.calls.filter(c => c.method === 'DELETE').length, 1);
  assert.equal(JSON.parse(m.calls.find(c => c.method === 'PATCH').body).draft, undefined);
  assert.equal(m.release.draft, true);
});

test('published release is refused before any mutation', async t => {
  const f = fixture(t);
  const m = mock({ draft: false });
  await assert.rejects(publish({ ...f, request: m.request }), /published/);
  assert.ok(m.calls.every(c => c.method === 'GET'));
});

test('stop refreshing if draft was published between asset uploads', async t => {
  const f = fixture(t);
  const m = mock({ assets: [] });
  const request = async (url, options) => {
    const response = await m.request(url, options);
    if (url.includes('uploads.github.com')) m.release.draft = false;
    return response;
  };
  await assert.rejects(publish({ ...f, request }), /published/);
  assert.equal(m.calls.filter(c => c.url.includes('uploads.github.com')).length, 1);
});

test('missing archive or bad checksum fails before calling GitHub', async t => {
  const f = fixture(t);
  const m = mock();
  const checksum = fs.readdirSync(f.directory).find(n => n.endsWith('.sha256'));
  fs.writeFileSync(path.join(f.directory, checksum), 'bad');
  await assert.rejects(publish({ ...f, request: m.request }), /Checksum/);
  fs.rmSync(path.join(f.directory, checksum));
  await assert.rejects(publish({ ...f, request: m.request }), /exactly three/);
  assert.equal(m.calls.length, 0);
});

test('a remote tag pointing to another commit is refused before mutation', async t => {
  const f = fixture(t);
  const m = mock();
  const request = async (url, options) => {
    const response = await m.request(url, options);
    if (url.includes('/git/tags/')) response.json = async () => ({ object: { type: 'commit', sha: 'another-commit' } });
    return response;
  };
  await assert.rejects(publish({ ...f, request }), /validated commit/);
  assert.ok(m.calls.every(c => c.method === 'GET'));
});
