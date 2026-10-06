import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';

if (process.platform === 'win32') {
  const sdk = process.env.VULKAN_SDK;
  for (const relative of ['Include/vulkan/vulkan.h', 'Lib/vulkan-1.lib', 'Bin/glslc.exe']) {
    if (!sdk || !fs.existsSync(path.join(sdk, relative))) throw new Error(`Incomplete Vulkan SDK: ${relative}`);
  }
  // The SDK's import library does not include the redistributable loader DLL.
  const version = '1.3.296.0';
  const name = `VulkanRT-${version}-Components`;
  const root = fs.mkdtempSync(path.join(process.env.RUNNER_TEMP || os.tmpdir(), 'plume-vulkan-'));
  const archive = path.join(root, `${name}.zip`);
  execFileSync('curl', ['--fail', '--location', '--retry', '3', `https://sdk.lunarg.com/sdk/download/${version}/windows/${name}.zip`, '--output', archive], { stdio: 'inherit' });
  const digest = createHash('sha256').update(fs.readFileSync(archive)).digest('hex');
  if (digest !== 'e0429f91cf356fe99e381f7d22e568ac81945669f1be1772d933a0d189230452') throw new Error('Vulkan runtime archive checksum mismatch');
  execFileSync('tar', ['-xf', archive, '-C', root]);
  fs.copyFileSync(path.join(root, name, 'x64', 'vulkan-1.dll'), path.join(sdk, 'Bin', 'vulkan-1.dll'));
  fs.copyFileSync(path.join(root, name, 'VulkanRT-License.txt'), path.join(sdk, 'Bin', 'VulkanRT-License.txt'));
  // Cargo tests run in debug/deps; DLLs copied beside plume live in debug.
  fs.appendFileSync(process.env.GITHUB_PATH, `${process.env.CARGO_TARGET_DIR || 'C:\\t'}\\debug\n`);
}
