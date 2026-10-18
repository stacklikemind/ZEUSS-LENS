#!/usr/bin/env python3
"""Package the single native executable for the current platform."""
import argparse
import os
from pathlib import Path
import platform
import plistlib
import shutil
import subprocess
import tarfile
import zipfile

root = Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser()
parser.add_argument('--binary', type=Path)
parser.add_argument('--arch')
args = parser.parse_args()
system = platform.system()
arch = args.arch or platform.machine().lower()
name = 'zeuss-lens'
binary = args.binary or root / 'desktop' / 'target' / 'release' / (name + ('.exe' if system == 'Windows' else ''))
if not binary.is_file():
    raise SystemExit(f'Build the release binary first: {binary}')
dist = root / 'dist'
dist.mkdir(exist_ok=True)
notices = '\n\n'.join((root / path).read_text(encoding="utf-8") for path in ['desktop/THIRD_PARTY.md', 'desktop/fonts/OFL.txt', 'desktop/licenses/Apache-2.0.txt', 'desktop/licenses/Lucide.txt'])
notice_path = dist / 'THIRD-PARTY-NOTICES.txt'
notice_path.write_text(notices, encoding="utf-8")
if system == 'Darwin':
    bundle = (dist if arch == platform.machine().lower() else dist / arch) / 'ZEUSS LENS.app'
    macos = bundle / 'Contents' / 'MacOS'
    macos.mkdir(parents=True, exist_ok=True)
    shutil.copy2(binary, macos / name)
    (macos / name).chmod(0o755)
    info = {'CFBundleExecutable': name, 'CFBundleIdentifier': 'dev.zeusslens.desktop', 'CFBundleName': 'ZEUSS LENS', 'CFBundleDisplayName': 'ZEUSS LENS', 'CFBundlePackageType': 'APPL', 'CFBundleShortVersionString': '0.2.0', 'CFBundleVersion': '2', 'NSHighResolutionCapable': True, 'LSMinimumSystemVersion': '12.0'}
    with (bundle / 'Contents' / 'Info.plist').open('wb') as f:
        plistlib.dump(info, f)
    resources = bundle / 'Contents' / 'Resources'
    resources.mkdir(exist_ok=True)
    (resources / 'THIRD-PARTY-NOTICES.txt').write_text(notices, encoding="utf-8")
    subprocess.run(['codesign', '--force', '--sign', '-', str(bundle)], check=True)
    archive = dist / f'{name}-macos-{arch}.zip'
    subprocess.run(['ditto', '-c', '-k', '--keepParent', str(bundle), str(archive)], check=True)
elif system == 'Windows':
    archive = dist / f'{name}-windows-{arch}.zip'
    with zipfile.ZipFile(archive, 'w', zipfile.ZIP_DEFLATED) as f:
        f.write(binary, name + '.exe')
        f.write(notice_path, 'THIRD-PARTY-NOTICES.txt')
else:
    archive = dist / f'{name}-linux-{arch}.tar.gz'
    with tarfile.open(archive, 'w:gz') as f:
        f.add(binary, arcname=name)
        f.add(notice_path, arcname='THIRD-PARTY-NOTICES.txt')
print(archive)
