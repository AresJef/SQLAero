"""Verify the actual release files and installed native identities for CI.

This utility does not build, run tests, sample performance, or publish. The
existing workflow owns those operations; this file keeps its shared identity
checks consistent between original wheels, sdist rebuilds, and PyPI downloads.
"""

from __future__ import annotations

import argparse
from email.parser import BytesParser
import hashlib
from importlib import import_module
from importlib.metadata import version
import json
from pathlib import Path
import platform
import subprocess
import sys
import tarfile
import tomllib
from typing import Any, cast
from xml.etree import ElementTree
import zipfile

ROOT = Path(__file__).resolve().parents[1]
PLATFORMS = ('macos-arm64', 'macos-x86_64', 'windows-x86_64', 'linux-x86_64')
PYTHONS = ('cp312', 'cp313', 'cp314', 'cp315')


def digest(path: Path) -> str:
    """Hash file contents without loading a distribution into Python memory."""
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def release_version() -> str:
    """Read the sole Python distribution version from the frozen checkout."""
    return str(tomllib.loads((ROOT / 'pyproject.toml').read_text(encoding='utf-8'))['project']['version'])


def wheel_record(path: Path) -> dict[str, Any]:
    """Check ABI, platform, metadata and the single native member of a wheel."""
    name, package_version, python, abi, tag = path.stem.split('-')
    assert name == 'sqlaero' and package_version == release_version(), path.name
    assert python in PYTHONS and abi == python, (python, abi)
    if tag.endswith('_arm64') and tag.startswith('macosx_'):
        target = 'macos-arm64'
    elif tag.endswith('_x86_64') and tag.startswith('macosx_'):
        target = 'macos-x86_64'
    elif tag == 'win_amd64':
        target = 'windows-x86_64'
    else:
        assert tag == 'manylinux_2_28_x86_64', tag
        target = 'linux-x86_64'
    with zipfile.ZipFile(path) as archive:
        native = [entry for entry in archive.namelist()
                  if entry.startswith('sqlaero/_sqlaero') and entry.endswith(('.so', '.pyd'))]
        metadata = [entry for entry in archive.namelist() if entry.endswith('.dist-info/METADATA')]
        assert len(native) == len(metadata) == 1, (native, metadata)
        parsed = BytesParser().parsebytes(archive.read(metadata[0]))
        assert parsed['Name'] == 'sqlaero' and parsed['Version'] == package_version
        native_hash = hashlib.sha256(archive.read(native[0])).hexdigest()
    return {'file': path.name, 'sha256': digest(path), 'bytes': path.stat().st_size,
            'version': package_version, 'python': python, 'platform': target,
            'native_member': native[0], 'native_sha256': native_hash}


def installed(wheel: Path, output: Path, kind: str, sdist: Path | None) -> None:
    """Require the current interpreter to load the native bytes in this wheel."""
    record = wheel_record(wheel)
    package = import_module('sqlaero')
    native = import_module('sqlaero._sqlaero')
    location = Path(cast(str, native.__file__)).resolve()
    package_location = Path(cast(str, package.__file__)).resolve()
    prefix = Path(sys.prefix).resolve()
    assert location.is_relative_to(prefix) and package_location.is_relative_to(prefix), location
    assert digest(location) == record['native_sha256'], location
    assert version('sqlaero') == record['version']
    assert record['python'] == f'cp{sys.version_info.major}{sys.version_info.minor}'
    machine = platform.machine().lower()
    assert machine in ({'arm64', 'aarch64'} if record['platform'] == 'macos-arm64'
                       else {'amd64', 'x86_64'}), machine
    record.update(kind=kind, executable=sys.executable, prefix=str(prefix),
                  package=str(package_location), native=str(location), python_version=sys.version,
                  machine=machine, operating_system=platform.platform(),
                  source_commit=subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
                  cargo_lock_sha256=digest(ROOT / 'Cargo.lock'))
    if sdist is not None:
        record['sdist_sha256'] = digest(sdist)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(record, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(record))


def extract(path: Path, destination: Path, expected: str) -> None:
    """Extract only the exact canonical sdist into a fresh independent directory."""
    assert digest(path) == expected, 'canonical sdist hash mismatch'
    assert not destination.exists(), destination
    destination.mkdir(parents=True)
    with tarfile.open(path) as archive:
        archive.extractall(destination, filter='data')
    roots = list(destination.iterdir())
    assert len(roots) == 1 and roots[0].is_dir(), roots
    assert (roots[0] / 'Cargo.lock').is_file()
    assert (roots[0] / 'third-party/jiter/src/lib.rs').is_file()
    print(roots[0].resolve().as_posix())


def test_totals(path: Path) -> dict[str, int]:
    """Read the actual JUnit outcome rather than inferring tests from filenames."""
    root = ElementTree.parse(path).getroot()
    suites = [root] if root.tag == 'testsuite' else list(root.findall('testsuite'))
    totals = {name: sum(int(suite.get(name, '0')) for suite in suites)
              for name in ('tests', 'failures', 'errors', 'skipped')}
    assert totals['tests'] > 0 and totals['failures'] == totals['errors'] == 0, (path, totals)
    return totals


def manifest(directory: Path, identities: Path, output: Path) -> None:
    """Gate the exact 16-wheel cross product and its independent sdist evidence."""
    files = sorted(path for path in directory.iterdir() if path.is_file())
    wheels = [wheel_record(path) for path in files if path.suffix == '.whl']
    sdists = [path for path in files if path.name.endswith('.tar.gz')]
    assert len(files) == 17 and len(wheels) == 16 and len(sdists) == 1, [p.name for p in files]
    assert sdists[0].name == f'sqlaero-{release_version()}.tar.gz'
    expected = {(target, python) for target in PLATFORMS for python in PYTHONS}
    assert {(str(row['platform']), str(row['python'])) for row in wheels} == expected
    source_hash = digest(sdists[0])
    evidence: list[dict[str, Any]] = []
    commits: set[str] = set()
    for kind in ('original', 'sdist'):
        paths = list(identities.rglob(f'{kind}-wheel-identity.json'))
        assert len(paths) == 16, (kind, paths)
        seen: set[tuple[str, str]] = set()
        for path in paths:
            record: dict[str, Any] = json.loads(path.read_text(encoding='utf-8'))
            key = (str(record['platform']), str(record['python']))
            assert key not in seen and key in expected and record['kind'] == kind
            seen.add(key)
            commits.add(str(record['source_commit']))
            assert record['sdist_sha256'] == source_hash, path
            if kind == 'original':
                match = next(row for row in wheels if (row['platform'], row['python']) == key)
                assert match['sha256'] == record['sha256'] and match['native_sha256'] == record['native_sha256']
            evidence.append({'kind': kind, 'platform': key[0], 'python': key[1],
                             'wheel_sha256': record['sha256'],
                             'tests': test_totals(path.with_name(f'python-{kind}.xml'))})
    assert len(commits) == 1, commits
    result = {'version': release_version(), 'source_commit': next(iter(commits)),
              'wheels': wheels, 'sdist': {'file': sdists[0].name, 'sha256': source_hash,
                                        'bytes': sdists[0].stat().st_size}, 'evidence': evidence}
    output.write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
    print(f'Validated 16 original wheels, one sdist and 32 installed test identities: {output}')


def main() -> None:
    """Expose the three identity operations needed by the existing workflow."""
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    identity = commands.add_parser('identity')
    identity.add_argument('--wheel', type=Path, required=True)
    identity.add_argument('--output', type=Path, required=True)
    identity.add_argument('--kind', choices=('original', 'sdist', 'pypi'), required=True)
    identity.add_argument('--sdist', type=Path)
    unpack = commands.add_parser('extract')
    unpack.add_argument('--sdist', type=Path, required=True)
    unpack.add_argument('--destination', type=Path, required=True)
    unpack.add_argument('--expected-sha', required=True)
    bundle = commands.add_parser('manifest')
    bundle.add_argument('--directory', type=Path, required=True)
    bundle.add_argument('--identities', type=Path, required=True)
    bundle.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if args.command == 'identity':
        installed(args.wheel, args.output, args.kind, args.sdist)
    elif args.command == 'extract':
        extract(args.sdist, args.destination, args.expected_sha)
    else:
        manifest(args.directory, args.identities, args.output)


if __name__ == '__main__':
    main()
