#!/usr/bin/env python3
"""Opt-in source installation/removal of the Linux tray helper only."""
import argparse
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import stat
import subprocess
import sys
import tempfile


def digest(data):
    return hashlib.sha256(data).hexdigest()


def regular(path):
    return path.exists() and stat.S_ISREG(path.lstat().st_mode)


def snapshot(path, limit=None):
    try:
        descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC)
    except FileNotFoundError:
        return None
    with os.fdopen(descriptor, 'rb') as stream:
        metadata = os.fstat(stream.fileno())
        if not stat.S_ISREG(metadata.st_mode):
            raise ValueError(f'Preserving a non-regular file: {path}')
        data = stream.read() if limit is None else stream.read(limit + 1)
        if limit is not None and len(data) > limit:
            raise ValueError(f'Tray file exceeds size limit: {path}')
        return data, stat.S_IMODE(metadata.st_mode)


def same_file(left, right):
    try:
        a, b = left.lstat(), right.lstat()
        return (a.st_dev, a.st_ino) == (b.st_dev, b.st_ino)
    except FileNotFoundError:
        return False


@contextmanager
def defer_interrupt():
    previous = signal.pthread_sigmask(signal.SIG_BLOCK, {signal.SIGINT})
    try:
        yield
    finally:
        signal.pthread_sigmask(signal.SIG_SETMASK, previous)


class FileChange:
    def __init__(self, path, expected, payload):
        self.path, self.expected, self.payload = path, expected, payload
        path.parent.mkdir(parents=True, exist_ok=True)
        self.directory = Path(tempfile.mkdtemp(prefix='.klicky-tray-', dir=path.parent))
        self.backup = self.directory / 'previous'
        self.staged = self.directory / 'new'
        self.probe = self.directory / 'link-test'
        self.displaced = self.directory / 'displaced'

    def apply(self):
        data, mode = self.payload if self.payload is not None else (b'', 0o600)
        with self.staged.open('xb') as stream:
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
        self.staged.chmod(mode)
        # Verify exclusive publication/recovery support before moving an old
        # file, including removal on an XDG home on another filesystem.
        os.link(self.staged, self.probe, follow_symlinks=False)
        self.probe.unlink()
        if self.expected is not None:
            # Capture the actual directory entry before inspecting it. A hash
            # check followed by os.replace would still overwrite a racing edit.
            os.rename(self.path, self.backup)
            if snapshot(self.backup) != self.expected:
                raise ValueError(f'Owned tray file changed during commit: {self.path}')
        if self.payload is not None:
            # link never replaces an entry created since capture/preflight.
            os.link(self.staged, self.path, follow_symlinks=False)

    def validate(self):
        if self.expected is not None and snapshot(self.backup) != self.expected:
            raise ValueError(f'Captured tray file changed during commit: {self.path}')
        if snapshot(self.path) != self.payload or (
            self.payload is not None and not same_file(self.path, self.staged)
        ):
            raise ValueError(f'Tray destination changed during commit: {self.path}')

    def restore(self, source):
        if os.path.lexists(source):
            try:
                os.link(source, self.path, follow_symlinks=False)
            except OSError:
                # Never overwrite another editor's replacement during recovery.
                print(f'Preserved recovery file: {source} (destination: {self.path})', file=sys.stderr)

    def rollback(self):
        if self.payload is not None and same_file(self.path, self.staged):
            os.rename(self.path, self.displaced)
            if same_file(self.displaced, self.staged) and snapshot(self.displaced) == self.payload:
                self.displaced.unlink()
            else:
                self.restore(self.displaced)
        self.restore(self.backup)

    def cleanup(self, committed):
        self.staged.unlink(missing_ok=True)
        self.probe.unlink(missing_ok=True)
        for saved in (self.backup, self.displaced):
            if not os.path.lexists(saved):
                continue
            if same_file(saved, self.path) or (
                committed and saved == self.backup and snapshot(saved) == self.expected
            ):
                saved.unlink()
        if not any(self.directory.iterdir()):
            self.directory.rmdir()
        else:
            print(f'Preserved tray recovery files in: {self.directory}', file=sys.stderr)


class FileTransaction:
    def __init__(self):
        self.changes = []
        self.unchanged = []

    def replace(self, path, expected, payload):
        # Journal before the first mutation, including interrupts immediately
        # after rename/link returns and before Python's next statement runs.
        with defer_interrupt():
            change = FileChange(path, expected, payload)
            self.changes.append(change)
        change.apply()

    def __enter__(self):
        return self

    def __exit__(self, kind, error, traceback):
        committed = False
        try:
            if kind is None:
                for change in self.changes:
                    change.validate()
                for path, expected in self.unchanged:
                    if snapshot(path) != expected:
                        raise ValueError(f'Tray destination changed during commit: {path}')
                committed = True
        finally:
            with defer_interrupt():
                if not committed:
                    for change in reversed(self.changes):
                        try:
                            change.rollback()
                        except (OSError, ValueError) as recovery_error:
                            print(f'Tray recovery incomplete: {recovery_error}', file=sys.stderr)
                for change in self.changes:
                    try:
                        change.cleanup(committed)
                    except (OSError, ValueError) as cleanup_error:
                        print(f'Preserved tray recovery files in {change.directory}: {cleanup_error}', file=sys.stderr)


def desktop_arg(value):
    # Desktop-entry strings are unescaped once before Exec argument parsing.
    value = str(value).replace('%', '%%').replace('\\', '\\\\\\\\')
    for character in ('"', '`', '$'):
        value = value.replace(character, '\\\\' + character)
    return '"' + value + '"'


def setup_lock_path(paths):
    # The binary has a fixed HOME path even when XDG homes change.
    return paths['binary'].with_name('.klicky-tray-install.lock')


def locations():
    home = Path.home()
    data = Path(os.environ.get('XDG_DATA_HOME') or home / '.local/share')
    config = Path(os.environ.get('XDG_CONFIG_HOME') or home / '.config')
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
    for path in [*paths.values(), manifest, setup_lock_path(paths)]:
        if any(c in str(path) for c in '\r\n\x00'):
            raise ValueError('Installation paths cannot contain line breaks or NUL')
        for ancestor in [path, *path.parents]:
            if ancestor.is_symlink():
                raise ValueError(f'Refusing a symlink in installation path: {ancestor}')
    return paths, manifest


@contextmanager
def installation_lock(paths):
    import fcntl
    path = setup_lock_path(paths)
    path.parent.mkdir(parents=True, exist_ok=True)
    # Never unlink this lock: all setup commands must use the same inode,
    # including after removal and while another process is acquiring it.
    descriptor = os.open(path, os.O_CREAT | os.O_RDWR | os.O_NOFOLLOW | os.O_CLOEXEC, 0o600)
    with os.fdopen(descriptor, 'rb+') as stream:
        metadata = os.fstat(stream.fileno())
        if not stat.S_ISREG(metadata.st_mode) or metadata.st_uid != os.getuid():
            raise ValueError('Tray setup lock must be a regular file owned by the current user')
        try:
            fcntl.flock(stream.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as error:
            raise ValueError('Another tray setup command is running; wait for it to finish and retry') from error
        yield


def read_manifest(manifest, paths):
    original = snapshot(manifest, limit=4096)
    if original is None:
        return {}, None
    record = json.loads(original[0])
    if not isinstance(record, dict):
        raise ValueError('Invalid tray installation record')
    files = record.get('files', {})
    if record.get('schema') != 1 or not isinstance(files, dict) or set(files) - paths.keys():
        raise ValueError('Unsupported tray installation record')
    if any(not isinstance(value, str) or len(value) != 64 for value in files.values()):
        raise ValueError('Invalid tray installation hashes')
    # Ownership hashes and the recovery precondition come from the same read.
    return files, original


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


def install(args, paths, manifest, ownership):
    previous, original_record = ownership
    root = Path(__file__).resolve().parent.parent
    cli = Path(args.cli or shutil.which('klicky') or Path.home() / '.local/bin/klicky').absolute()
    if not cli.is_file() or not os.access(cli, os.X_OK):
        raise ValueError('Install the Klicky engine first, or use --cli PATH')
    if any(c in str(cli) for c in '\r\n\x00'):
        raise ValueError('CLI path cannot contain line breaks or NUL')
    # Preflight the entire owned set before building or changing any file.
    old = check_owned(paths, previous)
    enabled = args.autostart or (old['autostart'] is not None and not args.no_autostart)
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
    # Retain the layout from our unchanged owned launcher on ordinary upgrades.
    flat_menu = args.flat_menu
    if flat_menu is None:
        flat_menu = any(
            line.startswith(b'Exec=') and line.endswith((b' "--flat-menu"', b' "--flat-menu" "--wait-for-tray"'))
            for name in ('desktop', 'autostart') if old[name] is not None
            for line in old[name][0].splitlines()
        )
    command_args = [launcher, '--', paths['binary'], '--cli', cli, '--icon', paths['icon']]
    if flat_menu:
        command_args.append('--flat-menu')
    command = ' '.join(desktop_arg(arg) for arg in command_args)
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
    if snapshot(manifest) != original_record:
        raise ValueError('Tray installation record changed during preparation')
    with FileTransaction() as transaction:
        if old['autostart'] is None and 'autostart' not in payloads:
            transaction.unchanged.append((paths['autostart'], None))
        for name, (data, mode) in payloads.items():
            transaction.replace(paths[name], old[name], (data, mode))
        if 'autostart' in previous and not enabled and paths['autostart'].exists():
            transaction.replace(paths['autostart'], old['autostart'], None)
        record = {'schema': 1, 'files': {name: digest(data) for name, (data, _) in payloads.items()}}
        transaction.replace(manifest, original_record, ((json.dumps(record, indent=2) + '\n').encode(), 0o600))
    print(f'Installed helper: {paths["binary"]}')
    print(f'Helper login startup: {"enabled" if enabled else "not enabled"}. No helper or engine was started.')
    print('Run the helper from the application menu. Use only one desktop controller if Klicky Bar is already enabled.')


def remove(paths, manifest, ownership):
    previous, original_record = ownership
    if snapshot(manifest) != original_record:
        raise ValueError('Tray installation record changed before removal')
    retained = {}
    removals = []
    for name, expected in previous.items():
        path = paths[name]
        try:
            current = snapshot(path)
        except ValueError:
            current = False
        if current is None:
            continue
        if current is False or digest(current[0]) != expected:
            print(f'Preserved a modified file: {path}', file=sys.stderr)
            retained[name] = expected
        else:
            removals.append((path, current))
    with FileTransaction() as transaction:
        for path, expected in removals:
            transaction.replace(path, expected, None)
        record = ((json.dumps({'schema': 1, 'files': retained}, indent=2) + '\n').encode(), 0o600) if retained else None
        transaction.replace(manifest, original_record, record)
    if retained:
        raise ValueError('Modified tray files remain. Back them up and resolve them before reinstalling.')
    if manifest.parent.exists() and not any(manifest.parent.iterdir()):
        manifest.parent.rmdir()
    print('Removed owned tray-helper files only. Engine installation and settings are unchanged.')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', help='Use an already built helper instead of building with Cargo')
    parser.add_argument('--cli', help='Path to the existing Klicky CLI')
    layout = parser.add_mutually_exclusive_group()
    layout.add_argument('--flat-menu', dest='flat_menu', action='store_true', help='Use one menu on hosts with clipped submenus')
    layout.add_argument('--submenus', dest='flat_menu', action='store_false', help='Restore the normal submenu layout')
    parser.set_defaults(flat_menu=None)
    startup = parser.add_mutually_exclusive_group()
    startup.add_argument('--autostart', action='store_true', help='Explicitly enable helper-only login startup')
    startup.add_argument('--no-autostart', action='store_true', help='Remove owned helper login startup')
    parser.add_argument('--remove', action='store_true', help='Remove only unchanged files recorded by this installer; quit the helper first')
    args = parser.parse_args()
    if sys.platform != 'linux':
        parser.error('This installer supports Linux only')
    paths, manifest = locations()
    with installation_lock(paths):
        paths, manifest = locations()
        ownership = read_manifest(manifest, paths)
        if args.remove:
            remove(paths, manifest, ownership)
        else:
            install(args, paths, manifest, ownership)


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        sys.exit(f'Tray setup stopped: {error}')
