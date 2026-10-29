"""The requested release matrix is independent of the wheel parser's tables.

Synthetic members test metadata selection only. Native loading and real
architecture/libc compatibility are checked by each installed CI interpreter.
"""

from pathlib import Path
from zipfile import ZipFile

import pytest

from tools import release_artifacts


@pytest.mark.parametrize('python', ['cp312', 'cp313', 'cp314', 'cp315'])
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
    tmp_path: Path, python: str, tag: str, platform: str,
) -> None:
    version = release_artifacts.release_version()
    wheel = tmp_path / f'sqlaero-{version}-{python}-{python}-{tag}.whl'
    with ZipFile(wheel, 'w') as archive:
        archive.writestr('sqlaero/_sqlaero.pyd' if tag.startswith('win_')
                         else 'sqlaero/_sqlaero.so', b'metadata fixture, not executable code')
        archive.writestr(f'sqlaero-{version}.dist-info/METADATA',
                         f'Metadata-Version: 2.4\nName: sqlaero\nVersion: {version}\n')
    record = release_artifacts.wheel_record(wheel)
    assert (record['platform'], record['python']) == (platform, python)


@pytest.mark.parametrize('tag', ['win32', 'manylinux_2_17_aarch64',
                                'musllinux_1_1_x86_64', 'macosx_12_0_arm64'])
def test_other_platform_baselines_are_not_silently_accepted(tmp_path: Path, tag: str) -> None:
    version = release_artifacts.release_version()
    with pytest.raises(AssertionError):
        release_artifacts.wheel_record(tmp_path / f'sqlaero-{version}-cp313-cp313-{tag}.whl')
