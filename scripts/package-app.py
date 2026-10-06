#!/usr/bin/env python3
"""Build a native Plume package with its application icon (Python stdlib only)."""

import argparse
import os
from pathlib import Path
import plistlib
import re
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
APP = ROOT / "crates" / "stt-app"
BRAND = APP / "assets" / "brand"


def package(profile, skip_build):
    platform = {"darwin": "macos", "win32": "windows", "linux": "linux"}.get(sys.platform)
    if platform is None:
        raise RuntimeError(f"Unsupported platform: {sys.platform}")
    if not skip_build:
        subprocess.run(
            ["cargo", "build", "-p", "stt-app", "--bin", "plume", "--profile", profile],
            cwd=ROOT, check=True,
        )
    target = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
    if not target.is_absolute():
        target = ROOT / target
    binary = target / ("debug" if profile == "dev" else profile) / (
        "plume.exe" if platform == "windows" else "plume"
    )
    if not binary.is_file():
        raise FileNotFoundError(f"Build Plume first: {binary}")
    destination = target / "package" / ("Plume.app" if platform == "macos" else f"plume-{platform}")
    destination.mkdir(parents=True, exist_ok=True)
    if platform == "macos":
        contents = destination / "Contents"
        (contents / "MacOS").mkdir(parents=True, exist_ok=True)
        (contents / "Resources").mkdir(parents=True, exist_ok=True)
        shutil.copy2(binary, contents / "MacOS" / "plume")
        shutil.copy2(BRAND / "plume.icns", contents / "Resources" / "plume.icns")
        with (APP / "packaging" / "Info.plist").open("rb") as source:
            info = plistlib.load(source)
        version = re.search(r'^version\s*=\s*"([^"]+)"', (APP / "Cargo.toml").read_text(), re.M).group(1)
        info.update(CFBundleShortVersionString=version, CFBundleVersion=version)
        with (contents / "Info.plist").open("wb") as output:
            plistlib.dump(info, output)
        # Distribution signing and notarization require publisher credentials.
        subprocess.run(["codesign", "--force", "--sign", "-", str(destination)], check=True)
    elif platform == "windows":
        loader = binary.parent / "vulkan-1.dll"
        if not loader.is_file():
            raise FileNotFoundError(f"Missing required Vulkan loader: {loader}")
        shutil.copy2(binary, destination / binary.name)
        shutil.copy2(loader, destination / loader.name)
        shutil.copy2(BRAND / "plume.ico", destination / "plume.ico")
    else:
        bin_dir = destination / "bin"
        applications = destination / "share" / "applications"
        theme = destination / "share" / "icons" / "hicolor"
        icons = theme / "512x512" / "apps"
        scalable = theme / "scalable" / "apps"
        for directory in (bin_dir, applications, icons, scalable):
            directory.mkdir(parents=True, exist_ok=True)
        shutil.copy2(binary, bin_dir / "plume")
        shutil.copy2(APP / "packaging" / "plume.desktop", applications / "plume.desktop")
        shutil.copy2(BRAND / "plume-linux.png", icons / "plume.png")
        shutil.copy2(BRAND / "plume-app.svg", scalable / "plume.svg")
    return destination


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile", choices=("dev", "release"), default="release")
    parser.add_argument("--skip-build", action="store_true", help="Package an existing build")
    args = parser.parse_args()
    print(package(args.profile, args.skip_build))


if __name__ == "__main__":
    main()
