// Assemble native binaries, close their dynamic dependency graph, relocate it,
// then test the actual extracted archive with a minimal OS-only environment.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { workspace } from '../release.mjs';

function run(cmd, args, options = {}) {
  const result = spawnSync(cmd, args, { encoding: 'utf8', ...options });
  if (result.error || result.status !== 0) throw new Error(`${cmd} ${args.join(' ')}: ${result.error?.message ?? result.stderr ?? result.stdout}`);
  return result.stdout;
}
function walk(dir) {
  if (!dir || !fs.existsSync(dir)) return [];
  return fs.readdirSync(dir, { withFileTypes: true }).flatMap(e => {
    const f = path.join(dir, e.name);
    return e.isDirectory() ? walk(f) : [f];
  });
}
const platform = process.platform;
const targets = {
  'linux-x64': 'x86_64-unknown-linux-gnu',
  'win32-x64': 'x86_64-pc-windows-msvc',
  'darwin-x64': 'x86_64-apple-darwin',
  'darwin-arm64': 'aarch64-apple-darwin',
};
const target = targets[`${platform}-${process.arch}`];
if (!target) throw new Error('Unsupported native runner');
const { version } = workspace();
const build = path.resolve(process.env.CARGO_TARGET_DIR || 'target', 'release');
const dist = path.resolve('dist');
const name = `stt-v${version}-${target}`;
const stage = path.join(dist, name);
if (fs.existsSync(stage)) throw new Error(`Output already exists: ${stage}`);
fs.mkdirSync(stage, { recursive: true });
const extension = platform === 'win32' ? '.exe' : '';
const executableSources = ['plume', 'stt-shell', 'plume-updater'].map(n => path.join(build, n + extension));
const libraryPattern = platform === 'win32' ? /\.dll$/i : platform === 'darwin' ? /\.dylib$/ : /\.so(?:\.\d+)*$/;
const candidates = new Map();
function register(files) {
  for (const f of files.filter(f => libraryPattern.test(f))) {
    const key = path.basename(f).toLowerCase();
    if (!candidates.has(key)) candidates.set(key, f);
  }
}
// ort's copy-dylibs feature places runtime providers beside the executables,
// sometimes as symlinks into its download cache. copyFile dereferences them.
register(walk(build));
if (platform === 'win32') {
  register(walk(process.env.VCToolsRedistDir && path.join(process.env.VCToolsRedistDir, 'x64')));
  register(walk(process.env.VULKAN_SDK && path.join(process.env.VULKAN_SDK, 'Bin')));
  register(walk(process.env.VULKAN_SDK && path.join(process.env.VULKAN_SDK, 'Runtime')));
}
const copied = new Map();
function copy(source, basename = path.basename(source)) {
  if (platform === 'win32') basename = basename.toLowerCase();
  const real = fs.realpathSync(source);
  if (copied.has(basename)) {
    if (copied.get(basename) !== real && !fs.readFileSync(source).equals(fs.readFileSync(path.join(stage, basename)))) throw new Error(`Conflicting runtime libraries: ${basename}`);
    return;
  }
  fs.copyFileSync(real, path.join(stage, basename));
  if (platform !== 'win32') fs.chmodSync(path.join(stage, basename), 0o755);
  copied.set(basename, real);
}
executableSources.forEach(f => copy(f));
// Include libraries loaded by provider code, even if absent from the import
// table. Only top-level runtime libraries are seeds, not compiler build tools.
for (const f of fs.readdirSync(build).filter(n => libraryPattern.test(n))) copy(path.join(build, f));

// glibc and the ELF loader must remain supplied by the OS. Other Linux runtime
// libraries (ALSA, XCB, XKB, C++ etc.) are bundled recursively.
const linuxSystem = /^(?:ld-linux.*|lib(?:c|m|dl|pthread|rt|resolv|util)\.so(?:\.\d+)*)$/;
const windowsRuntime = /^(?:vcruntime|msvcp|concrt|vcomp|directml|vulkan-1)/i;
// Explicit OS imports: a DLL merely being installed in System32 does not make
// it an OS prerequisite (SDKs and third-party installers put runtimes there).
const windowsOsLibraries = new Set(`advapi32 avrt bcrypt bcryptprimitives cabinet cfgmgr32 comctl32 comdlg32 crypt32 cryptbase cryptsp d3d11 d3d12 d3dcompiler_47 dbghelp dbgcore dcomp dnsapi dsound dwmapi dwrite dxcore dxgi gdi32 hid icuuc imm32 iphlpapi kernel32 kernelbase mf mfplat mfreadwrite mfuuid mpr msvcrt mswsock ncrypt netapi32 normaliz ntdll ole32 oleaut32 opengl32 powrprof propsys psapi rpcrt4 sechost secur32 setupapi shell32 shcore shlwapi ucrtbase user32 userenv usp10 uxtheme version winhttp wininet winmm winspool wintrust winusb wldap32 ws2_32 wtsapi32`.split(' ').map(n => n + '.dll'));
function windowsSystem(name) {
  if (windowsRuntime.test(name)) return false;
  return /^(?:api-ms-win-|ext-ms-win-)/i.test(name) || windowsOsLibraries.has(name.toLowerCase());
}
function macSystem(name) { return name.startsWith('/usr/lib/') || name.startsWith('/System/Library/'); }
function rpaths(file) {
  return [...run('otool', ['-l', file]).matchAll(/cmd LC_RPATH\s+cmdsize \d+\s+path (.+?) \(offset/g)].map(m => m[1]);
}
function macResolve(dep, source) {
  const expand = p => p.replace('@loader_path', path.dirname(source)).replace('@executable_path', build);
  const possibilities = dep.startsWith('@rpath/')
    ? [...rpaths(source), ...executableSources.flatMap(rpaths)].map(p => path.join(expand(p), dep.slice(7)))
    : [expand(dep)];
  possibilities.push(candidates.get(path.basename(dep).toLowerCase()));
  const resolved = possibilities.find(f => f && fs.existsSync(f));
  if (!resolved) throw new Error(`Unresolved macOS runtime: ${dep} in ${source}`);
  return resolved;
}
function dependencies(file) {
  if (platform === 'win32') {
    return [...run('dumpbin', ['/dependents', file]).matchAll(/^\s+([^\s]+\.dll)\s*$/gim)]
      .map(m => m[1]).filter(n => !windowsSystem(n)).map(n => {
        const systemRuntime = path.join(process.env.SystemRoot, 'System32', n);
        const source = candidates.get(n.toLowerCase()) || (windowsRuntime.test(n) && fs.existsSync(systemRuntime) ? systemRuntime : undefined);
        if (!source) throw new Error(`Unresolved Windows runtime: ${n}`);
        return { reference: n, source, basename: n };
      });
  }
  if (platform === 'darwin') {
    return run('otool', ['-L', file]).split('\n').slice(1).map(l => l.trim().split(' (')[0]).filter(Boolean)
      .filter(n => !macSystem(n))
      // A dylib's first entry can be its own install ID.
      .filter(n => !libraryPattern.test(file) || path.basename(n) !== path.basename(file))
      .map(n => ({ reference: n, source: macResolve(n, file), basename: path.basename(n) }));
  }
  const output = run('ldd', [file], { env: { ...process.env, LD_LIBRARY_PATH: build } });
  if (/=> not found/.test(output)) throw new Error(`Missing Linux runtime:\n${output}`);
  return [...output.matchAll(/^\s*(\S+) => (\/\S+) /gm)].filter(m => !linuxSystem.test(m[1]))
    .map(m => ({ reference: m[1], source: m[2], basename: m[1] }));
}
// Resolve against original files while their original loader paths still work.
const graph = new Map();
for (const [basename, source] of copied) {
  const deps = dependencies(source);
  graph.set(basename, deps);
  for (const dep of deps) copy(dep.source, dep.basename);
}
for (const [basename, deps] of graph) {
  const file = path.join(stage, basename);
  if (platform === 'linux') run('patchelf', ['--set-rpath', '$ORIGIN', file]);
  if (platform === 'darwin') {
    if (libraryPattern.test(basename)) run('install_name_tool', ['-id', `@loader_path/${basename}`, file]);
    for (const dep of deps) run('install_name_tool', ['-change', dep.reference, `@loader_path/${dep.basename}`, file]);
    // All bundled references now use @loader_path. Remove absolute build rpaths.
    for (const p of new Set(rpaths(file))) run('install_name_tool', ['-delete_rpath', p, file]);
  }
}
if (platform === 'darwin') {
  // Mach-O edits invalidate linker ad-hoc signatures, required on Apple Silicon.
  // This uses no identity/certificate and is not publisher signing/notarization.
  for (const basename of [...copied.keys()].sort((a, b) => Number(!libraryPattern.test(a)) - Number(!libraryPattern.test(b)))) {
    run('codesign', ['--force', '--sign', '-', path.join(stage, basename)]);
    run('codesign', ['--verify', '--strict', path.join(stage, basename)]);
  }
}
fs.copyFileSync('README.md', path.join(stage, 'README.md'));
if (fs.existsSync('docs')) fs.cpSync('docs', path.join(stage, 'docs'), { recursive: true });
const brand = path.join(stage, 'crates/stt-app/assets/brand');
fs.mkdirSync(brand, { recursive: true });
fs.copyFileSync('crates/stt-app/assets/brand/plume.png', path.join(brand, 'plume.png'));
fs.copyFileSync(`releases/v${version}.md`, path.join(stage, 'RELEASE-NOTES.md'));
fs.writeFileSync(path.join(stage, 'RUNTIME-LIBRARIES.txt'), [...copied.keys()].filter(n => libraryPattern.test(n)).join('\n') + '\n');
fs.writeFileSync(path.join(stage, 'UNSIGNED.txt'), 'These archives are not publisher-signed or notarized. macOS binaries use only local ad-hoc signatures after relocation. Models are downloaded separately. See README.md.\n');
if (platform === 'darwin') {
  const app = path.join(stage, 'Plume.app');
  const contents = path.join(app, 'Contents');
  const macos = path.join(contents, 'MacOS');
  fs.mkdirSync(macos, { recursive: true });
  fs.mkdirSync(path.join(contents, 'Resources'));
  for (const basename of copied.keys()) {
    if (basename === 'stt-shell') continue;
    fs.copyFileSync(path.join(stage, basename), path.join(macos, basename));
    fs.chmodSync(path.join(macos, basename), 0o755);
  }
  fs.copyFileSync('crates/stt-app/assets/brand/plume.icns', path.join(contents, 'Resources', 'plume.icns'));
  fs.copyFileSync('crates/stt-app/packaging/Info.plist', path.join(contents, 'Info.plist'));
  for (const key of ['CFBundleShortVersionString', 'CFBundleVersion']) run('/usr/libexec/PlistBuddy', ['-c', `Set :${key} ${version}`, path.join(contents, 'Info.plist')]);
  run('codesign', ['--force', '--sign', '-', app]);
  run('codesign', ['--verify', '--deep', '--strict', app]);
}
// Preserve license notices supplied alongside native runtimes where available.
const licenseDir = path.join(stage, 'runtime-licenses');
for (const [basename, source] of copied) {
  if (!libraryPattern.test(basename)) continue;
  for (const directory of [path.dirname(source), path.dirname(path.dirname(source))]) {
    for (const f of fs.readdirSync(directory).filter(n => /license|notice|copyright/i.test(n))) {
      const sourceFile = path.join(directory, f);
      if (fs.statSync(sourceFile).isFile()) {
        fs.mkdirSync(licenseDir, { recursive: true });
        fs.copyFileSync(sourceFile, path.join(licenseDir, `${basename}-${f}`));
      }
    }
  }
}
const archive = path.join(dist, name + (platform === 'win32' ? '.zip' : '.tar.gz'));
run('tar', [platform === 'win32' ? '-a' : '-z', '-cf', archive, '-C', dist, name]);
const digest = createHash('sha256').update(fs.readFileSync(archive)).digest('hex');
fs.writeFileSync(archive + '.sha256', `${digest}  ${path.basename(archive)}\n`);

const extracted = fs.mkdtempSync(path.join(os.tmpdir(), 'stt-archive-'));
try {
  run('tar', ['-xf', archive, '-C', extracted]);
  const root = path.join(extracted, name);
  const systemPath = platform === 'win32' ? `${process.env.SystemRoot}\\System32;${process.env.SystemRoot}` : '/usr/bin:/bin';
  const env = { PATH: systemPath, HOME: extracted, TMPDIR: extracted, TEMP: extracted, TMP: extracted, LANG: 'C.UTF-8' };
  if (platform === 'win32') Object.assign(env, { SystemRoot: process.env.SystemRoot, WINDIR: process.env.SystemRoot });
  // Audit every ELF, including stt-app and runtime providers; no original cache
  // paths may resolve. System libraries are restricted to the glibc allowlist.
  if (platform === 'linux') {
    for (const basename of copied.keys()) {
      const output = run('ldd', [path.join(root, basename)], { cwd: extracted, env });
      if (/not found/.test(output)) throw new Error(`Extracted runtime missing: ${output}`);
      for (const m of output.matchAll(/^\s*(\S+) => (\/\S+) /gm)) {
        if (!linuxSystem.test(m[1]) && !m[2].startsWith(root + path.sep)) throw new Error(`Runtime escaped archive: ${m[2]}`);
      }
    }
  }
  if (platform === 'darwin') {
    const nativeApp = path.join(root, 'Plume.app', 'Contents', 'MacOS');
    for (const basename of copied.keys()) {
      for (const directory of basename === 'stt-shell' ? [root] : [root, nativeApp]) {
        const file = path.join(directory, basename);
        for (const line of run('otool', ['-L', file]).split('\n').slice(1)) {
          const dep = line.trim().split(' (')[0];
          if (dep && !macSystem(dep) && (!dep.startsWith('@loader_path/') || !fs.existsSync(path.join(directory, dep.slice(13))))) throw new Error(`Runtime escaped archive: ${dep}`);
        }
      }
    }
  }
  if (platform === 'win32') {
    const entries = new Set(fs.readdirSync(root).map(n => n.toLowerCase()));
    for (const basename of copied.keys()) {
      for (const m of run('dumpbin', ['/dependents', path.join(root, basename)]).matchAll(/^\s+([^\s]+\.dll)\s*$/gim)) {
        if (!windowsSystem(m[1]) && !entries.has(m[1].toLowerCase())) throw new Error(`Missing extracted runtime: ${m[1]}`);
      }
    }
  }
  const output = run(path.join(root, 'stt-shell' + extension), ['--help'], { cwd: extracted, env });
  if (!output.includes(`stt-shell ${version}`) && !/usage|transcribe/i.test(output)) throw new Error('stt-shell --help did not print the expected version or usage');
  const helper = run(path.join(root, 'plume-updater' + extension), ['--help'], { cwd: extracted, env });
  if (!helper.includes(`Plume updater ${version}`)) throw new Error('Bundled updater could not start');
  console.log(output);
  console.log(`Archive verified: ${archive}\nSHA-256: ${digest}`);
} finally {
  fs.rmSync(extracted, { recursive: true, force: true });
}
