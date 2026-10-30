"""The requested release matrix is independent of the wheel parser's tables.

Synthetic members test metadata selection only. Native loading and real
architecture/libc compatibility are checked by each installed CI interpreter.
"""

import hashlib
import json
from pathlib import Path
import subprocess
from zipfile import ZipFile

import pytest

from tools import release_artifacts

# Frozen independently from cbest05-cp313-full.xml, not the tool's allowlist.
PARALLEL_NAMES = (
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
)
PARALLEL_REASON = ('Parallel performance observations require POSIX resource snapshots '
                   'and are measured on macOS')
UNIX_NAME = 'test_windows_unix_socket_is_rejected_without_tcp_fallback'
UNIX_REASON = 'Windows explicitly rejects the Unix-only transport'


def write_skips(path: Path, names: tuple[str, ...], classname: str, reason: str) -> None:
    from xml.etree import ElementTree

    suite = ElementTree.Element('testsuite', tests=str(max(1, len(names))), failures='0',
                               errors='0', skipped=str(len(names)))
    for name in names:
        case = ElementTree.SubElement(suite, 'testcase', classname=classname, name=name)
        ElementTree.SubElement(case, 'skipped', message=reason)
    ElementTree.ElementTree(suite).write(path, encoding='utf-8')


@pytest.mark.parametrize(('python', 'abi'), [
    ('cp312', 'cp312'), ('cp313', 'cp313'), ('cp314', 'cp314'), ('cp315', 'cp315'),
    ('cp314', 'cp314t'), ('cp315', 'cp315t'),
])
@pytest.mark.parametrize(('tag', 'platform'), [
    ('macosx_11_0_arm64', 'macos-arm64'),
    ('macosx_11_0_x86_64', 'macos-x86_64'),
    ('win_amd64', 'windows-x86_64'),
    ('win_arm64', 'windows-arm64'),
    ('manylinux_2_28_x86_64', 'linux-glibc-x86_64'),
    ('manylinux_2_28_aarch64', 'linux-glibc-aarch64'),
    ('musllinux_1_2_x86_64', 'linux-musl-x86_64'),
    ('musllinux_1_2_aarch64', 'linux-musl-aarch64'),
])
def test_requested_wheel_tag_maps_to_one_platform(
    tmp_path: Path, python: str, abi: str, tag: str, platform: str,
) -> None:
    version = release_artifacts.release_version()
    wheel = tmp_path / f'sqlaero-{version}-{python}-{abi}-{tag}.whl'
    with ZipFile(wheel, 'w') as archive:
        archive.writestr('sqlaero/_sqlaero.pyd' if tag.startswith('win_')
                         else 'sqlaero/_sqlaero.so', b'metadata fixture, not executable code')
        archive.writestr(f'sqlaero-{version}.dist-info/METADATA',
                         f'Metadata-Version: 2.4\nName: sqlaero\nVersion: {version}\n')
    record = release_artifacts.wheel_record(wheel)
    assert (record['platform'], record['python'], record['abi']) == (platform, python, abi)


@pytest.mark.parametrize(('python', 'abi'), [
    ('cp313', 'cp313t'), ('cp314', 'cp315t'), ('cp315t', 'cp315t'),
    ('cp314', 'abi3t'), ('cp315', 'cp315td'),
])
def test_unapproved_or_mismatched_python_abis_are_rejected(
    tmp_path: Path, python: str, abi: str,
) -> None:
    version = release_artifacts.release_version()
    with pytest.raises(AssertionError):
        release_artifacts.wheel_record(
            tmp_path / f'sqlaero-{version}-{python}-{abi}-macosx_11_0_arm64.whl')


@pytest.mark.parametrize('tag', ['win32', 'manylinux_2_17_aarch64',
                                'musllinux_1_1_x86_64', 'macosx_12_0_arm64'])
def test_other_platform_baselines_are_not_silently_accepted(tmp_path: Path, tag: str) -> None:
    version = release_artifacts.release_version()
    with pytest.raises(AssertionError):
        release_artifacts.wheel_record(tmp_path / f'sqlaero-{version}-cp313-cp313-{tag}.whl')


@pytest.mark.parametrize('matches_manifest', [True, False])
def test_pypi_download_requires_the_exact_tested_bytes(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, matches_manifest: bool,
) -> None:
    version = release_artifacts.release_version()
    name = f'sqlaero-{version}-cp313-cp313-musllinux_1_2_aarch64.whl'
    destination = tmp_path / 'download'
    manifest = tmp_path / 'manifest.json'
    manifest.write_text(json.dumps({'version': version, 'wheels': [{
        'file': name, 'sha256': hashlib.sha256(b'tested bytes').hexdigest(),
    }]}))

    def pip_download(command: list[str], *, check: bool) -> subprocess.CompletedProcess[str]:
        assert '--only-binary=:all:' in command and f'sqlaero=={version}' in command
        assert not check
        (destination / name).write_bytes(b'tested bytes' if matches_manifest else b'other bytes')
        return subprocess.CompletedProcess(command, 0)

    monkeypatch.setattr(release_artifacts.subprocess, 'run', pip_download)
    output = tmp_path / 'identity.json'
    if matches_manifest:
        release_artifacts.download(version, manifest, destination, output)
        assert json.loads(output.read_text())['file'] == name
    else:
        with pytest.raises(AssertionError, match='differs from the tested bundle'):
            release_artifacts.download(version, manifest, destination, output)
        assert not output.exists()


@pytest.mark.parametrize('reason', ['explicit isolated native MySQL fixture',
                                  'requires explicitly selected owned MySQL fixtures'])
def test_missing_real_database_prerequisites_do_not_pass_release_evidence(
    tmp_path: Path, reason: str,
) -> None:
    junit = tmp_path / 'python-original.xml'
    junit.write_text('<testsuite tests="1" failures="0" errors="0" skipped="1">'
                     '<testcase name="test_real_database">'
                     f'<skipped message="{reason}"/></testcase></testsuite>')
    with pytest.raises(AssertionError, match='Unexpected skipped requirement'):
        release_artifacts.test_totals(junit)


@pytest.mark.parametrize('target', ['windows-x86_64', 'windows-arm64'])
def test_exact_parallel_na_is_accepted_only_for_windows(tmp_path: Path, target: str) -> None:
    junit = tmp_path / 'windows.xml'
    write_skips(junit, PARALLEL_NAMES, 'tests.test_benchmark_parallel', PARALLEL_REASON)
    assert release_artifacts.test_totals(junit, target=target)['skipped'] == 20


@pytest.mark.parametrize('target', [None, 'macos-arm64', 'linux-glibc-x86_64', 'linux-musl-aarch64'])
def test_parallel_na_is_rejected_without_a_windows_record(tmp_path: Path, target: str | None) -> None:
    junit = tmp_path / 'other.xml'
    write_skips(junit, PARALLEL_NAMES, 'tests.test_benchmark_parallel', PARALLEL_REASON)
    with pytest.raises(AssertionError, match='Unexpected skipped requirement'):
        release_artifacts.test_totals(junit, target=target)


@pytest.mark.parametrize(('name', 'classname', 'reason'), [
    ('test_parallel_unknown', 'tests.test_benchmark_parallel', PARALLEL_REASON),
    ('test_real_database', 'tests.test_public_connection', PARALLEL_REASON),
    (PARALLEL_NAMES[0], 'tests.test_public_connection', PARALLEL_REASON),
    (PARALLEL_NAMES[0], 'tests.test_benchmark_parallel', 'requires owned MySQL fixtures'),
    (PARALLEL_NAMES[0], 'tests.test_benchmark_parallel', PARALLEL_REASON + ' changed'),
    (UNIX_NAME, 'tests.test_public_connection', UNIX_REASON),
])
def test_unknown_or_mislabeled_windows_skip_is_rejected(
    tmp_path: Path, name: str, classname: str, reason: str,
) -> None:
    junit = tmp_path / 'bad.xml'
    write_skips(junit, (name,), classname, reason)
    with pytest.raises(AssertionError, match='Unexpected skipped requirement'):
        release_artifacts.test_totals(junit, target='windows-arm64')


def test_duplicate_na_cannot_replace_another_required_case(tmp_path: Path) -> None:
    junit = tmp_path / 'duplicate.xml'
    write_skips(junit, (PARALLEL_NAMES[0],) * 2, 'tests.test_benchmark_parallel', PARALLEL_REASON)
    with pytest.raises(AssertionError, match='Duplicate skipped requirement'):
        release_artifacts.test_totals(junit, target='windows-x86_64')


@pytest.mark.parametrize('target', [None, 'macos-arm64', 'linux-glibc-aarch64'])
def test_exact_unix_socket_na_remains_accepted(tmp_path: Path, target: str | None) -> None:
    junit = tmp_path / 'unix.xml'
    write_skips(junit, (UNIX_NAME,), 'tests.test_public_connection', UNIX_REASON)
    assert release_artifacts.test_totals(junit, target=target)['skipped'] == 1


@pytest.mark.parametrize(('windows_count', 'unix_count', 'accepted'), [
    (20, 1, True), (19, 1, False), (20, 0, False),
])
def test_manifest_requires_platform_specific_na_counts(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch,
    windows_count: int, unix_count: int, accepted: bool,
) -> None:
    version = release_artifacts.release_version()
    distribution = tmp_path / 'dist'
    distribution.mkdir()
    sdist = distribution / f'sqlaero-{version}.tar.gz'
    sdist.write_bytes(b'canonical source fixture')
    identities = tmp_path / 'identities'
    records: dict[str, dict[str, object]] = {}
    monkeypatch.setattr(release_artifacts, 'PLATFORMS', ('windows-x86_64', 'macos-arm64'))
    monkeypatch.setattr(release_artifacts, 'PYTHONS', ('cp313',))
    for target in ('windows-x86_64', 'macos-arm64'):
        filename = f'{target}.whl'
        (distribution / filename).write_bytes(b'wheel fixture')
        row: dict[str, object] = {'file': filename, 'platform': target, 'python': 'cp313',
                                  'abi': 'cp313', 'sha256': 'wheel', 'native_sha256': 'native'}
        records[filename] = row
        folder = identities / target
        folder.mkdir(parents=True)
        for kind in ('original', 'sdist'):
            identity = {**row, 'kind': kind, 'source_commit': 'frozen-source',
                        'sdist_sha256': release_artifacts.digest(sdist),
                        'free_threaded': False, 'gil_enabled': True}
            (folder / f'{kind}-wheel-identity.json').write_text(json.dumps(identity))
            names = PARALLEL_NAMES[:windows_count] if target.startswith('windows-') else (UNIX_NAME,)[:unix_count]
            write_skips(folder / f'python-{kind}.xml', names,
                        'tests.test_benchmark_parallel' if target.startswith('windows-')
                        else 'tests.test_public_connection',
                        PARALLEL_REASON if target.startswith('windows-') else UNIX_REASON)

    def wheel_record(path: Path) -> dict[str, object]:
        return records[path.name]

    monkeypatch.setattr(release_artifacts, 'wheel_record', wheel_record)
    output = tmp_path / 'manifest.json'
    if accepted:
        release_artifacts.manifest(distribution, identities, output)
        result = json.loads(output.read_text())
        assert {row['platform']: row['tests']['skipped'] for row in result['evidence']} == {
            'windows-x86_64': 20, 'macos-arm64': 1}
    else:
        with pytest.raises(AssertionError):
            release_artifacts.manifest(distribution, identities, output)
        assert not output.exists()
