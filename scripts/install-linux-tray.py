#!/usr/bin/env python3
"""Opt-in source installation/removal of the Linux tray helper only."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys
import tempfile


def digest(data):
    return hashlib.sha256(data).hexdigest()


def regular(path):
    return path.exists() and stat.S_ISREG(path.lstat().st_mode)


def atomic(path, data, mode):
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(dir=path.parent, delete=False) as stream:
            temporary = Path(stream.name)
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
        temporary.chmod(mode)
        os.replace(temporary, path)
    finally:
        if temporary is not None and temporary.exists():
            temporary.unlink()


def desktop_arg(value):
    # Desktop-entry strings are unescaped once before Exec argument parsing.
    value = str(value).replace('%', '%%').replace('\\', '\\\\\\\\')
    for character in ('"', '`', '$'):
        value = value.replace(character, '\\\\' + character)
    return '"' + value + '"'


def locations():
    home = Path.home()
    data = Path(os.environ.get('XDG_DATA_HOME', str(home / '.local/share')))
    config = Path(os.environ.get('XDG_CONFIG_HOME', str(home / '.config')))
    if not data.is_absolute() or not config.is_absolute():
        raise ValueError('XDG_DATA_HOME and XDG_CONFIG_HOME must be absolute paths')
    folder = data / 'klicky-tray'
    desktop = 'io.github.muneer320.klicky-tray.desktop'
    paths = {
        'binary': home / '.local/bin/klicky-tray',
        'icon': folder / 'klicky.png',
        'desktop': data / 'applications' / desktop,
        'autostart': config / 'autostart' / desktop,
    }
    manifest = folder / 'installation.json'
    for path in [*paths.values(), manifest]:
        if any(c in str(path) for c in '\r\n\x00'):
            raise ValueError('Installation paths cannot contain line breaks or NUL')
        for ancestor in [path, *path.parents]:
            if ancestor.is_symlink():
                raise ValueError(f'Refusing a symlink in installation path: {ancestor}')
    return paths, manifest


def read_manifest(manifest, paths):
    if not manifest.exists():
        return {}
    if not regular(manifest):
        raise ValueError('Tray installation record is not a regular file')
    if manifest.stat().st_size > 4096:
        raise ValueError('Tray installation record is too large')
    record = json.loads(manifest.read_text())
    if not isinstance(record, dict):
        raise ValueError('Invalid tray installation record')
    files = record.get('files', {})
    if record.get('schema') != 1 or not isinstance(files, dict) or set(files) - paths.keys():
        raise ValueError('Unsupported tray installation record')
    if any(not isinstance(value, str) or len(value) != 64 for value in files.values()):
        raise ValueError('Invalid tray installation hashes')
    return files


def check_owned(paths, previous):
    snapshot = {}
    for name, path in paths.items():
        if path.exists():
            if name not in previous or not regular(path):
                raise ValueError(f'Preserving an unowned or modified file: {path}')
            data = path.read_bytes()
            if digest(data) != previous[name]:
                raise ValueError(f'Preserving an unowned or modified file: {path}')
            snapshot[name] = (data, stat.S_IMODE(path.stat().st_mode))
        else:
            snapshot[name] = None
    return snapshot


def install(args, paths, manifest, previous):
    root = Path(__file__).resolve().parent.parent
    cli = Path(args.cli or shutil.which('klicky') or Path.home() / '.local/bin/klicky').absolute()
    if not cli.is_file() or not os.access(cli, os.X_OK):
        raise ValueError('Install the Klicky engine first, or use --cli PATH')
    if any(c in str(cli) for c in '\r\n\x00'):
        raise ValueError('CLI path cannot contain line breaks or NUL')
    enabled = args.autostart or ('autostart' in previous and not args.no_autostart)
    # Preflight the entire owned set before building or changing any file.
    old = check_owned(paths, previous)
    original_record = manifest.read_bytes() if manifest.exists() else None
    if args.binary:
        binary = Path(args.binary)
    else:
        build = subprocess.run(['cargo', 'build', '--release', '--locked', '--message-format=json-render-diagnostics', '--manifest-path', str(root / 'packaging/linux-tray/Cargo.toml')], check=True, stdout=subprocess.PIPE, text=True)
        artifacts = [json.loads(line) for line in build.stdout.splitlines()]
        executables = [item['executable'] for item in artifacts if item.get('reason') == 'compiler-artifact' and item.get('target', {}).get('name') == 'klicky-tray' and 'bin' in item.get('target', {}).get('kind', []) and item.get('executable')]
        if len(executables) != 1:
            raise ValueError('Cargo did not report exactly one tray-helper executable')
        binary = Path(executables[0])
    # Gio checks Exec's first token before expanding %% field escapes. Use env
    # as the fixed executable so a literal percent in HOME still launches.
    launcher = Path('/usr/bin/env')
    if not launcher.is_file():
        raise ValueError('Desktop launchers require /usr/bin/env')
    command = ' '.join(desktop_arg(arg) for arg in (launcher, '--', paths['binary'], '--cli', cli, '--icon', paths['icon']))
    icon = str(paths['icon']).replace('\\', '\\\\')
    entry = ('[Desktop Entry]\nType=Application\nName=Klicky controls\n'
             'Comment=Control Klicky playback; quitting this helper keeps playback running\n'
             f'Exec={command}\nIcon={icon}\nTerminal=false\nCategories=AudioVideo;Audio;\n'
             'X-Klicky-Owner=linux-tray-v1\n')
    payloads = {
        'binary': (binary.read_bytes(), 0o755),
        'icon': ((root / 'assets/branding/klicky.png').read_bytes(), 0o644),
        'desktop': (entry.encode(), 0o644),
    }
    if enabled:
        payloads['autostart'] = (entry.replace(f'Exec={command}\n', f'Exec={command} "--wait-for-tray"\n').encode(), 0o644)
    # Cargo can take minutes. A desktop editor or another installer may have
    # changed the owned set while we built/read payloads. Do not authorize that
    # edit by using it as a new backup immediately before replacement.
    locations()  # Recheck symlink ancestors as well as destination contents.
    if check_owned(paths, previous) != old:
        raise ValueError('Owned tray files changed during preparation')
    if (manifest.read_bytes() if manifest.exists() else None) != original_record:
        raise ValueError('Tray installation record changed during preparation')
    written = []
    try:
        for name, (data, mode) in payloads.items():
            atomic(paths[name], data, mode)
            written.append(name)
        if 'autostart' in previous and not enabled and paths['autostart'].exists():
            paths['autostart'].unlink()
            written.append('autostart')
        record = {'schema': 1, 'files': {name: digest(data) for name, (data, _) in payloads.items()}}
        atomic(manifest, (json.dumps(record, indent=2) + '\n').encode(), 0o600)
    except Exception:
        for name in reversed(written):
            path = paths[name]
            expected = payloads.get(name)
            if path.exists() and (not regular(path) or expected is None or digest(path.read_bytes()) != digest(expected[0])):
                print(f'Preserved a concurrently changed file during rollback: {path}', file=sys.stderr)
                continue
            if old[name] is None:
                path.unlink(missing_ok=True)
            else:
                atomic(path, *old[name])
        raise
    print(f'Installed helper: {paths["binary"]}')
    print(f'Helper login startup: {"enabled" if enabled else "not enabled"}. No helper or engine was started.')
    print('Run the helper from the application menu. Use only one desktop controller if Klicky Bar is already enabled.')


def remove(paths, manifest, previous):
    retained = {}
    for name, expected in previous.items():
        path = paths[name]
        if not path.exists():
            continue
        if not regular(path) or digest(path.read_bytes()) != expected:
            print(f'Preserved a modified file: {path}', file=sys.stderr)
            retained[name] = expected
        else:
            path.unlink()
    if retained:
        atomic(manifest, (json.dumps({'schema': 1, 'files': retained}, indent=2) + '\n').encode(), 0o600)
        raise ValueError('Modified tray files remain. Back them up and resolve them before reinstalling.')
    manifest.unlink(missing_ok=True)
    if manifest.parent.exists() and not any(manifest.parent.iterdir()):
        manifest.parent.rmdir()
    print('Removed owned tray-helper files only. Engine installation and settings are unchanged.')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', help='Use an already built helper instead of building with Cargo')
    parser.add_argument('--cli', help='Path to the existing Klicky CLI')
    startup = parser.add_mutually_exclusive_group()
    startup.add_argument('--autostart', action='store_true', help='Explicitly enable helper-only login startup')
    startup.add_argument('--no-autostart', action='store_true', help='Remove owned helper login startup')
    parser.add_argument('--remove', action='store_true', help='Remove only unchanged files recorded by this installer; quit the helper first')
    args = parser.parse_args()
    if sys.platform != 'linux':
        parser.error('This installer supports Linux only')
    paths, manifest = locations()
    previous = read_manifest(manifest, paths)
    if args.remove:
        remove(paths, manifest, previous)
    else:
        install(args, paths, manifest, previous)


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        sys.exit(f'Tray setup stopped: {error}')
