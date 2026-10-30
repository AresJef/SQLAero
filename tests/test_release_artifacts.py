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
