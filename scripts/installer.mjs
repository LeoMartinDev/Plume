import fs from 'node:fs';
import path from 'node:path';

export const targets = ['x86_64-unknown-linux-gnu', 'x86_64-pc-windows-msvc', 'aarch64-apple-darwin'];
export function installerName(version, target) {
  const extension = target.includes('windows') ? 'exe' : target.includes('apple') ? 'dmg' : 'deb';
  if (!targets.includes(target)) throw new Error(`Unsupported installer target: ${target}`);
  return `Plume-v${version}-${target}.${extension}`;
}

// The dependency closure is already relocated in stage. Only application files
// are installed: models, history and preferences stay in the user's data folder.
export function buildInstaller({ platform, version, target, stage, dist, run, appId }) {
  const installer = path.join(dist, installerName(version, target));
  const packaging = path.resolve('crates/plume-app/packaging');
  if (platform === 'win32') {
    const iscc = process.env.ISCC || 'C:/Program Files (x86)/Inno Setup 6/ISCC.exe';
    run(iscc, [`/DAppVersion=${version}`, `/DSourceDir=${stage}`, `/DOutputDir=${dist}`, `/DOutputName=${path.basename(installer, '.exe')}`, ...(appId ? [`/DInstallerAppId=${appId}`, `/DAppGroupName=${appId}`] : []), path.join(packaging, 'windows.iss')]);
  } else if (platform === 'darwin') {
    const root = path.join(dist, 'dmg-root');
    fs.mkdirSync(root, { recursive: true });
    fs.cpSync(path.join(stage, 'Plume.app'), path.join(root, 'Plume.app'), { recursive: true });
    fs.symlinkSync('/Applications', path.join(root, 'Applications'));
    fs.copyFileSync(path.join(packaging, 'dmg-layout.ds-store'), path.join(root, '.DS_Store'));
    run('hdiutil', ['create', '-volname', 'Plume', '-srcfolder', root, '-format', 'UDZO', installer]);
  } else {
    const root = path.join(dist, 'deb-root');
    fs.mkdirSync(path.join(root, 'opt'), { recursive: true });
    fs.cpSync(stage, path.join(root, 'opt/plume'), { recursive: true });
    for (const directory of ['DEBIAN', 'usr/bin', 'usr/share/applications', 'usr/share/icons/hicolor/512x512/apps']) fs.mkdirSync(path.join(root, directory), { recursive: true });
    fs.writeFileSync(path.join(root, 'DEBIAN/control'), `Package: plume\nVersion: ${version}\nArchitecture: amd64\nMaintainer: LeoMartinDev <LeoMartinDev@users.noreply.github.com>\nDepends: libc6 (>= 2.39), pkexec\nSection: sound\nPriority: optional\nDescription: Local desktop voice dictation\n Dictate offline with local speech models. Requires an X11 desktop and system tray.\n`);
    fs.symlinkSync('/opt/plume/plume', path.join(root, 'usr/bin/plume'));
    fs.copyFileSync(path.join(packaging, 'plume.desktop'), path.join(root, 'usr/share/applications/plume.desktop'));
    fs.copyFileSync('crates/plume-app/assets/brand/plume-linux.png', path.join(root, 'usr/share/icons/hicolor/512x512/apps/plume.png'));
    run('dpkg-deb', ['--root-owner-group', '--build', root, installer]);
    run('dpkg-deb', ['--info', installer]);
  }
  return installer;
}

export function extractInstaller({ platform, installer, destination, run }) {
  if (platform === 'win32') {
    const root = path.join(destination, 'Plume');
    // Exercise the actual install wizard engine, including shortcuts/uninstall.
    run(installer, ['/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/SP-', `/DIR=${root}`, '/TASKS=']);
    return root;
  }
  if (platform === 'linux') {
    run('dpkg-deb', ['--extract', installer, destination]);
    return path.join(destination, 'opt/plume');
  }
  const mount = path.join(destination, 'dmg-mount');
  const app = path.join(destination, 'Plume.app');
  fs.mkdirSync(mount, { recursive: true });
  run('hdiutil', ['attach', '-readonly', '-nobrowse', '-noautoopen', '-mountpoint', mount, installer]);
  try {
    if (fs.readlinkSync(path.join(mount, 'Applications')) !== '/Applications') throw new Error('DMG is missing its Applications shortcut');
    if (!fs.existsSync(path.join(mount, '.DS_Store'))) throw new Error('DMG is missing its Finder layout');
    // Exercise the same bundle copy as dragging the app into Applications.
    run('ditto', [path.join(mount, 'Plume.app'), app]);
  } finally {
    run('hdiutil', ['detach', mount]);
  }
  return path.join(app, 'Contents/MacOS');
}
