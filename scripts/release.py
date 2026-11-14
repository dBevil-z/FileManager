#!/usr/bin/env python3
"""Build/validate the public assets and static Tauri update feed for one tag."""
import argparse
import datetime
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent
REPOSITORY = 'https://github.com/dBevil-z/FileManager'


def version_for(tag):
    if not re.fullmatch(r'v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)', tag):
        raise ValueError('Release tag must be a stable vMAJOR.MINOR.PATCH')
    return tag[1:]


def validate_versions(tag, root=ROOT):
    version = version_for(tag)
    for name in ['package.json', 'src-tauri/tauri.conf.json']:
        if json.loads((root / name).read_text())['version'] != version:
            raise ValueError(f'{name} version does not match {tag}')
    for name in ['src-tauri/Cargo.toml', 'crates/filemanager-core/Cargo.toml']:
        import tomllib
        if tomllib.loads((root / name).read_text())['package']['version'] != version:
            raise ValueError(f'{name} version does not match {tag}')
    return version


def names(version):
    return {
        'deb': f'FileManager_{version}_amd64.deb',
        'appimage': f'FileManager_{version}_amd64.AppImage',
        'user': f'FileManager_{version}_linux_x86_64.bin',
    }


def make_manifest(tag, signatures, notes, date=None):
    version = version_for(tag)
    files = names(version)
    platforms = {}
    for kind, name in files.items():
        signature = signatures[kind].strip()
        if not signature:
            raise ValueError(f'Missing signature for {kind}')
        platforms[f'linux-x86_64-{kind}'] = {
            'url': f'{REPOSITORY}/releases/download/{tag}/{name}',
            'signature': signature,
        }
    # Standard target for older Tauri clients; managed installations use explicit targets.
    platforms['linux-x86_64'] = platforms['linux-x86_64-appimage'].copy()
    return {
        'version': version, 'notes': notes,
        'pub_date': date or datetime.datetime.now(datetime.timezone.utc).isoformat().replace('+00:00', 'Z'),
        'platforms': platforms,
    }


def digest(path):
    h = hashlib.sha256()
    with path.open('rb') as stream:
        while chunk := stream.read(1024 * 1024):
            h.update(chunk)
    return h.hexdigest()


def prepare(tag):
    version = validate_versions(tag)
    files = names(version)
    output = ROOT / 'target/release/bundle/publish' / tag
    output.mkdir(parents=True, exist_ok=True)
    sources = {
        'deb': ROOT / 'target/release/bundle/deb' / files['deb'],
        'appimage': ROOT / 'target/release/bundle/appimage' / files['appimage'],
        'user': ROOT / 'target/release/filemanager',
    }
    signatures = {}
    for kind, source in sources.items():
        if not source.is_file():
            raise ValueError(f'Build artifact not found: {source}')
        destination = output / files[kind]
        shutil.copyfile(source, destination)
        destination.chmod(0o755 if kind in ('appimage', 'user') else 0o644)
        # Always sign with the version binding, including the raw user-install binary.
        # Signing credentials are read by Tauri from the environment, never logged here.
        result = subprocess.run(['pnpm', 'tauri', 'signer', 'sign', '--app-version', version, str(destination)],
                                cwd=ROOT, capture_output=True, text=True)
        if result.returncode:
            raise ValueError(f'Signing failed for {kind}; check the protected signing environment')
        signatures[kind] = destination.with_name(destination.name + '.sig').read_text().strip()
    body = ROOT / 'docs/releases' / f'{tag}.md'
    notes = body.read_text() if body.is_file() else f'FileManager {version}'
    manifest = make_manifest(tag, signatures, notes)
    (output / 'latest.json').write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + '\n')
    assets = sorted([*files.values(), *(name + '.sig' for name in files.values()), 'latest.json'])
    (output / 'SHA256SUMS').write_text(''.join(f'{digest(output / name)}  {name}\n' for name in assets))
    verify(tag, output)
    print(output)


def verify(tag, directory):
    version = version_for(tag)
    expected = set(names(version).values())
    expected |= {name + '.sig' for name in list(expected)} | {'latest.json'}
    if {p.name for p in directory.iterdir()} != expected | {'SHA256SUMS'}:
        raise ValueError('Unexpected or missing release asset')
    checksums = {}
    for line in (directory / 'SHA256SUMS').read_text().splitlines():
        hash_value, name = line.split('  ', 1)
        if name in checksums or name not in expected or not re.fullmatch('[a-f0-9]{64}', hash_value):
            raise ValueError('Invalid, duplicate or unexpected checksum entry')
        checksums[name] = hash_value
    if set(checksums) != expected:
        raise ValueError('Release asset or checksum missing')
    for name, hash_value in checksums.items():
        if digest(directory / name) != hash_value:
            raise ValueError(f'Checksum mismatch: {name}')
    manifest = json.loads((directory / 'latest.json').read_text())
    if manifest['version'] != version:
        raise ValueError('Update manifest version differs from release tag')
    canonical = make_manifest(tag, {kind: (directory / (name + '.sig')).read_text() for kind, name in names(version).items()}, manifest['notes'], manifest['pub_date'])
    if manifest != canonical:
        raise ValueError('Update manifest targets, URLs or signatures differ from release assets')
    print(f'Verified {tag}: 3 installers, 3 signatures, update feed and SHA256SUMS', file=sys.stderr)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command', choices=['check-version', 'prepare', 'verify'])
    parser.add_argument('--tag', required=True)
    parser.add_argument('--directory', type=Path)
    args = parser.parse_args()
    try:
        if args.command == 'check-version':
            print(validate_versions(args.tag))
        elif args.command == 'prepare':
            prepare(args.tag)
        else:
            verify(args.tag, args.directory or ROOT / 'target/release/bundle/publish' / args.tag)
    except (ValueError, OSError, KeyError) as error:
        parser.exit(1, f'{error}\n')


if __name__ == '__main__':
    main()
