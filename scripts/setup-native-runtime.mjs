// Intel macOS is no longer included in ort's default binary downloads.
// Pin the last official Intel runtime and verify it before extraction.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';

function env(name, value) {
  if (!process.env.GITHUB_ENV) throw new Error('Run this setup inside GitHub Actions');
  fs.appendFileSync(process.env.GITHUB_ENV, `${name}=${value}\n`);
}

if (process.platform === 'darwin' && process.arch === 'x64') {
  const version = '1.22.0';
  const name = `onnxruntime-osx-x86_64-${version}`;
  const root = fs.mkdtempSync(path.join(process.env.RUNNER_TEMP || os.tmpdir(), 'plume-ort-'));
  const archive = path.join(root, `${name}.tgz`);
  execFileSync('curl', ['--fail', '--location', '--retry', '3', `https://github.com/microsoft/onnxruntime/releases/download/v${version}/${name}.tgz`, '--output', archive], { stdio: 'inherit' });
  const digest = createHash('sha256').update(fs.readFileSync(archive)).digest('hex');
  if (digest !== 'e4ec94a7696de74fb1b12846569aa94e499958af6ffa186022cfde16c9d617f0') throw new Error('ONNX Runtime archive checksum mismatch');
  execFileSync('tar', ['-xf', archive, '-C', root]);
  env('ORT_LIB_LOCATION', path.join(root, name, 'lib'));
  env('ORT_PREFER_DYNAMIC_LINK', '1');
  env('RUSTFLAGS', '-C link-arg=-Wl,-rpath,@loader_path -C link-arg=-Wl,-headerpad_max_install_names');
}

if (process.platform === 'win32') {
  const sdk = process.env.VULKAN_SDK;
  for (const relative of ['Include/vulkan/vulkan.h', 'Lib/vulkan-1.lib', 'Bin/glslc.exe']) {
    if (!sdk || !fs.existsSync(path.join(sdk, relative))) throw new Error(`Incomplete Vulkan SDK: ${relative}`);
  }
  // Cargo tests run in debug/deps; DLLs copied beside plume live in debug.
  fs.appendFileSync(process.env.GITHUB_PATH, `${process.env.CARGO_TARGET_DIR || 'C:\\t'}\\debug\n`);
}
