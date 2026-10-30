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
import sysconfig
import tarfile
import time
import tomllib
from typing import Any, cast
from xml.etree import ElementTree
import zipfile

ROOT = Path(__file__).resolve().parents[1]
PLATFORM_TAGS = {
    'macos-arm64': 'macosx_11_0_arm64',
    'macos-x86_64': 'macosx_11_0_x86_64',
    'windows-x86_64': 'win_amd64',
    'windows-arm64': 'win_arm64',
    'linux-glibc-x86_64': 'manylinux_2_28_x86_64',
    'linux-glibc-aarch64': 'manylinux_2_28_aarch64',
    'linux-musl-x86_64': 'musllinux_1_2_x86_64',
    'linux-musl-aarch64': 'musllinux_1_2_aarch64',
}
PLATFORMS = tuple(PLATFORM_TAGS)
TAG_PLATFORMS = {tag: target for target, tag in PLATFORM_TAGS.items()}
PYTHONS = ('cp312', 'cp313', 'cp314', 'cp315')
RUNTIMES = tuple((python, python) for python in PYTHONS) + (
    ('cp314', 'cp314t'), ('cp315', 'cp315t'),
)
WINDOWS_PARALLEL_NA = frozenset({
    'test_parallel_cli_rejects_invalid_requests_before_import_or_output[case_ids0-extra0]',
    'test_parallel_cli_rejects_invalid_requests_before_import_or_output[case_ids1-extra1]',
    'test_parallel_cli_rejects_invalid_requests_before_import_or_output[case_ids2-extra2]',
    'test_parallel_cli_rejects_invalid_requests_before_import_or_output[case_ids3-extra3]',
    'test_parallel_cli_rejects_invalid_requests_before_import_or_output[case_ids4-extra4]',
    'test_parallel_cli_rejects_invalid_requests_before_import_or_output[case_ids5-extra5]',
    'test_parallel_cli_rejects_invalid_requests_before_import_or_output[case_ids6-extra6]',
    'test_parallel_cli_rejects_invalid_requests_before_import_or_output[case_ids7-extra7]',
    'test_parallel_cli_rejects_invalid_requests_before_import_or_output[case_ids8-extra8]',
    'test_parallel_cli_rejects_invalid_requests_before_import_or_output[case_ids9-extra9]',
    'test_parallel_cli_rejects_invalid_requests_before_import_or_output[case_ids10-extra10]',
    'test_parallel_cli_rejects_invalid_requests_before_import_or_output[case_ids11-extra11]',
    'test_parallel_cli_rejects_invalid_requests_before_import_or_output[case_ids12-extra12]',
    'test_parallel_cli_rejects_invalid_requests_before_import_or_output[case_ids13-extra13]',
    'test_parallel_cli_rejects_invalid_requests_before_import_or_output[case_ids14-extra14]',
    'test_free_threaded_entry_rejects_enabled_gil_before_loading_package',
    'test_parallel_consumptive_tasks_are_private_and_cleanup_follows_stop',
    'test_parallel_worker_failure_aborts_peers_and_closes_sessions[preparation]',
    'test_parallel_worker_failure_aborts_peers_and_closes_sessions[call]',
    'test_parallel_timeout_releases_barriers_and_reports_in_flight_work',
})


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
    assert (python, abi) in RUNTIMES, (python, abi)
    assert tag in TAG_PLATFORMS, tag
    target = TAG_PLATFORMS[tag]
    with zipfile.ZipFile(path) as archive:
        native = [entry for entry in archive.namelist()
                  if entry.startswith('sqlaero/_sqlaero') and entry.endswith(('.so', '.pyd'))]
        metadata = [entry for entry in archive.namelist() if entry.endswith('.dist-info/METADATA')]
        assert len(native) == len(metadata) == 1, (native, metadata)
        parsed = BytesParser().parsebytes(archive.read(metadata[0]))
        assert parsed['Name'] == 'sqlaero' and parsed['Version'] == package_version
        native_hash = hashlib.sha256(archive.read(native[0])).hexdigest()
    return {'file': path.name, 'sha256': digest(path), 'bytes': path.stat().st_size,
            'version': package_version, 'python': python, 'abi': abi, 'platform': target,
            'native_member': native[0], 'native_sha256': native_hash}


def installed(wheel: Path, output: Path, kind: str, sdist: Path | None) -> None:
    """Require the current interpreter to load the native bytes in this wheel."""
    # Public packaging tags inspect the actual interpreter and ELF loader.
    # This distinguishes musl from glibc and native ARM64 from x64 emulation.
    # packaging is an existing pytest dependency, never a product dependency.
    from packaging.tags import Tag, sys_tags

    record = wheel_record(wheel)
    assert Tag(str(record['python']), str(record['abi']),
               PLATFORM_TAGS[str(record['platform'])]) in sys_tags(), wheel.name
    free_threaded = bool(sysconfig.get_config_var('Py_GIL_DISABLED'))
    assert str(record['abi']).endswith('t') == free_threaded, record
    gil_enabled = getattr(sys, '_is_gil_enabled', lambda: True)
    before_import = bool(gil_enabled())
    assert before_import is not free_threaded, 'Interpreter has an unexpected GIL mode'
    package = import_module('sqlaero')
    native = import_module('sqlaero._sqlaero')
    assert bool(gil_enabled()) == before_import, 'Extension import changed the GIL mode'
    location = Path(cast(str, native.__file__)).resolve()
    package_location = Path(cast(str, package.__file__)).resolve()
    prefix = Path(sys.prefix).resolve()
    assert location.is_relative_to(prefix) and package_location.is_relative_to(prefix), location
    assert digest(location) == record['native_sha256'], location
    assert version('sqlaero') == record['version']
    assert record['python'] == f'cp{sys.version_info.major}{sys.version_info.minor}'
    machine = platform.machine().lower()
    assert machine in ({'arm64', 'aarch64'} if str(record['platform']).endswith(('arm64', 'aarch64'))
                       else {'amd64', 'x86_64'}), machine
    record.update(kind=kind, executable=sys.executable, prefix=str(prefix),
                  package=str(package_location), native=str(location), python_version=sys.version,
                  machine=machine, operating_system=platform.platform(),
                  python_platform=sysconfig.get_platform(), libc=platform.libc_ver(),
                  free_threaded=free_threaded, gil_enabled=bool(gil_enabled()),
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


def download(package_version: str, manifest_path: Path, destination: Path, output: Path) -> None:
    """Download this interpreter's PyPI wheel and require its tested hash.

    Index propagation receives the same six bounded attempts as the original
    release workflow. This operation never chooses an older version or builds
    a source fallback when the requested platform wheel is unavailable.
    """
    release: dict[str, Any] = json.loads(manifest_path.read_text(encoding='utf-8'))
    assert package_version == release['version'] == release_version()
    destination.mkdir(parents=True, exist_ok=True)
    for attempt in range(6):
        result = subprocess.run([
            sys.executable, '-m', 'pip', 'download', '--index-url', 'https://pypi.org/simple',
            '--no-cache-dir', '--no-deps', '--only-binary=:all:',
            f'sqlaero=={package_version}', '--dest', str(destination),
        ], check=False)
        if result.returncode == 0:
            break
        if attempt == 5:
            result.check_returncode()
        time.sleep(10)
    wheels = list(destination.glob('*.whl'))
    assert len(wheels) == 1, wheels
    wheel = wheels[0]
    record = next(row for row in release['wheels'] if row['file'] == wheel.name)
    assert digest(wheel) == record['sha256'], 'PyPI wheel differs from the tested bundle'
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps({'index': 'https://pypi.org/simple', 'file': wheel.name,
                                  'sha256': record['sha256']}, indent=2) + '\n', encoding='utf-8')


def test_totals(path: Path, target: str | None = None) -> dict[str, int]:
    """Read the actual JUnit outcome rather than inferring tests from filenames."""
    root = ElementTree.parse(path).getroot()
    suites = [root] if root.tag == 'testsuite' else list(root.findall('testsuite'))
    totals = {name: sum(int(suite.get(name, '0')) for suite in suites)
              for name in ('tests', 'failures', 'errors', 'skipped')}
    assert totals['tests'] > 0 and totals['failures'] == totals['errors'] == 0, (path, totals)
    windows = target in ('windows-x86_64', 'windows-arm64')
    seen: set[tuple[str | None, str | None]] = set()
    for case in root.iter('testcase'):
        skipped = case.find('skipped')
        if skipped is not None:
            identity = (case.get('classname'), case.get('name'))
            unix_socket = (not windows and identity == (
                'tests.test_public_connection',
                'test_windows_unix_socket_is_rejected_without_tcp_fallback')
                and skipped.get('message') == 'Windows explicitly rejects the Unix-only transport')
            parallel = (windows and identity[0] == 'tests.test_benchmark_parallel'
                        and identity[1] in WINDOWS_PARALLEL_NA
                        and skipped.get('message') == 'Parallel performance observations require '
                        'POSIX resource snapshots and are measured on macOS')
            assert unix_socket or parallel, (
                        'Unexpected skipped requirement', path, case.attrib, skipped.attrib)
            assert identity not in seen, ('Duplicate skipped requirement', path, identity)
            seen.add(identity)
    assert len(seen) == totals['skipped'], ('Skipped total differs from cases', path, totals)
    return totals


def manifest(directory: Path, identities: Path, output: Path, free_threaded: bool = False) -> None:
    """Gate the selected exact runtime matrix and independent sdist evidence."""
    runtimes = RUNTIMES if free_threaded else tuple((python, python) for python in PYTHONS)
    expected = {(target, python, abi) for target in PLATFORMS for python, abi in runtimes}
    files = sorted(path for path in directory.iterdir() if path.is_file())
    wheels = [wheel_record(path) for path in files if path.suffix == '.whl']
    sdists = [path for path in files if path.name.endswith('.tar.gz')]
    assert len(files) == len(expected) + 1 and len(wheels) == len(expected) and len(sdists) == 1, [p.name for p in files]
    assert sdists[0].name == f'sqlaero-{release_version()}.tar.gz'
    assert {(str(row['platform']), str(row['python']), str(row['abi'])) for row in wheels} == expected
    source_hash = digest(sdists[0])
    evidence: list[dict[str, Any]] = []
    commits: set[str] = set()
    for kind in ('original', 'sdist'):
        paths = list(identities.rglob(f'{kind}-wheel-identity.json'))
        assert len(paths) == len(expected), (kind, paths)
        seen: set[tuple[str, str, str]] = set()
        for path in paths:
            record: dict[str, Any] = json.loads(path.read_text(encoding='utf-8'))
            key = (str(record['platform']), str(record['python']), str(record['abi']))
            assert key not in seen and key in expected and record['kind'] == kind
            seen.add(key)
            commits.add(str(record['source_commit']))
            assert record['sdist_sha256'] == source_hash, path
            if kind == 'original':
                match = next(row for row in wheels if (row['platform'], row['python'], row['abi']) == key)
                assert match['sha256'] == record['sha256'] and match['native_sha256'] == record['native_sha256']
            totals = test_totals(path.with_name(f'python-{kind}.xml'), target=key[0])
            assert totals['skipped'] == (20 if key[0].startswith('windows-') else 1), (path, totals)
            assert bool(record['free_threaded']) == key[2].endswith('t')
            assert bool(record['gil_enabled']) is not bool(record['free_threaded'])
            evidence.append({'kind': kind, 'platform': key[0], 'python': key[1], 'abi': key[2],
                             'wheel_sha256': record['sha256'],
                             'tests': totals})
    assert len(commits) == 1, commits
    result = {'version': release_version(), 'source_commit': next(iter(commits)),
              'wheels': wheels, 'sdist': {'file': sdists[0].name, 'sha256': source_hash,
                                        'bytes': sdists[0].stat().st_size}, 'evidence': evidence}
    output.write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
    print(f'Validated {len(expected)} original wheels, one sdist and '
          f'{2 * len(expected)} installed test identities: {output}')


def main() -> None:
    """Expose shared identity operations used by the existing workflows."""
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
    bundle.add_argument('--free-threaded', action='store_true',
                        help='Require both approved free-threaded runtimes as well as ordinary GIL')
    fetch = commands.add_parser('download')
    fetch.add_argument('--version', required=True)
    fetch.add_argument('--manifest', type=Path, required=True)
    fetch.add_argument('--destination', type=Path, required=True)
    fetch.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if args.command == 'identity':
        installed(args.wheel, args.output, args.kind, args.sdist)
    elif args.command == 'extract':
        extract(args.sdist, args.destination, args.expected_sha)
    elif args.command == 'download':
        download(args.version, args.manifest, args.destination, args.output)
    else:
        manifest(args.directory, args.identities, args.output, args.free_threaded)


if __name__ == '__main__':
    main()
