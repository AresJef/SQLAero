"""The requested release matrix is independent of the wheel parser's tables.

Synthetic members test metadata selection only. Native loading and real
architecture/libc compatibility are checked by each installed CI interpreter.
"""

import hashlib
import json
from pathlib import Path
import subprocess
import io
import tarfile
from zipfile import ZipFile

import pytest

from tools import release_artifacts

# Frozen from collected test names, independently of the tool's allowlist.
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
    'test_parallel_binds_each_designated_input_and_keeps_shared_default',
    'test_parallel_factory_inputs_are_private_reused_and_checked_for_mutation',
    'test_cold_first_observation_precedes_checks_and_default_stays_hot_only',
    'test_cold_first_guards_and_original_creation_error_are_preserved',
)
PARALLEL_REASON = ('Parallel performance observations require POSIX resource snapshots '
                   'and are measured on macOS')
UNIX_NAME = 'test_windows_unix_socket_is_rejected_without_tcp_fallback'
UNIX_REASON = 'Windows explicitly rejects the Unix-only transport'


@pytest.fixture
def synthetic_license_materials(monkeypatch: pytest.MonkeyPatch) -> dict[str, bytes]:
    """Small independent bytes for wheel-format tests, not real legal materials."""
    materials = {'LICENSE': b'project license fixture\n',
                 'THIRD_PARTY_NOTICES.md': b'third-party inventory fixture\n',
                 'licenses/third-party/example-1.0.0/LICENSE': b'upstream copyright fixture\n'}
    monkeypatch.setattr(release_artifacts, 'license_materials', lambda: {
        name: hashlib.sha256(raw).hexdigest() for name, raw in materials.items()})
    return materials


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
    synthetic_license_materials: dict[str, bytes],
) -> None:
    version = release_artifacts.release_version()
    wheel = tmp_path / f'sqlaero-{version}-{python}-{abi}-{tag}.whl'
    with ZipFile(wheel, 'w') as archive:
        archive.writestr('sqlaero/_sqlaero.pyd' if tag.startswith('win_')
                         else 'sqlaero/_sqlaero.so', b'metadata fixture, not executable code')
        archive.writestr(f'sqlaero-{version}.dist-info/METADATA',
                         f'Metadata-Version: 2.4\nName: sqlaero\nVersion: {version}\n'
                         + ''.join(f'License-File: {name}\n' for name in synthetic_license_materials))
        for name, raw in synthetic_license_materials.items():
            archive.writestr(f'sqlaero-{version}.dist-info/licenses/{name}', raw)
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
    assert release_artifacts.test_totals(junit, target=target)['skipped'] == 24


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
    (24, 1, True), (23, 1, False), (24, 0, False),
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

    def sdist_record(path: Path) -> dict[str, object]:
        return {'file': path.name, 'sha256': release_artifacts.digest(path),
                'bytes': path.stat().st_size, 'license_file_count': 3}

    monkeypatch.setattr(release_artifacts, 'wheel_record', wheel_record)
    monkeypatch.setattr(release_artifacts, 'sdist_record', sdist_record)
    output = tmp_path / 'manifest.json'
    if accepted:
        release_artifacts.manifest(distribution, identities, output)
        result = json.loads(output.read_text())
        assert {row['platform']: row['tests']['skipped'] for row in result['evidence']} == {
            'windows-x86_64': 24, 'macos-arm64': 1}
    else:
        with pytest.raises(AssertionError):
            release_artifacts.manifest(distribution, identities, output)
        assert not output.exists()


def test_known_provider_collection_gap_needs_its_actual_capability_report(tmp_path: Path) -> None:
    from xml.etree import ElementTree
    report = tmp_path / 'tests.xml'
    write_skips(report, (UNIX_NAME,), 'tests.test_public_connection', UNIX_REASON)
    tree = ElementTree.parse(report)
    suite = tree.getroot()
    suite.set('tests', '2')
    suite.set('skipped', '2')
    case = ElementTree.SubElement(suite, 'testcase', classname='', name='tests.test_polars_input')
    ElementTree.SubElement(case, 'skipped', message='collection skipped')
    tree.write(report)
    with pytest.raises(AssertionError):
        release_artifacts.test_totals(report, 'macos-arm64')
    capabilities = tmp_path / 'providers.json'
    capabilities.write_text(json.dumps({'platform': 'macos-arm64', 'free_threaded': True,
        'unavailable': {'polars': 'NO_FIXED_POLARS_RUNTIME_FREE_THREADED_ABI'}}))
    assert release_artifacts.test_totals(report, 'macos-arm64', capabilities)['skipped'] == 2
    collection = suite.find("testcase[@name='tests.test_polars_input']")
    assert collection is not None
    collection.set('name', 'tests.test_pandas_input')
    tree.write(report)
    with pytest.raises(AssertionError):
        release_artifacts.test_totals(report, 'macos-arm64', capabilities)


def test_provider_gap_cannot_relax_a_gil_runtime_or_baseline_requirement(tmp_path: Path) -> None:
    report = tmp_path / 'tests.xml'
    write_skips(report, (UNIX_NAME,), 'tests.test_public_connection', UNIX_REASON)
    capabilities = tmp_path / 'providers.json'
    capabilities.write_text(json.dumps({'platform': 'macos-arm64', 'free_threaded': False,
        'unavailable': {'polars': 'NO_FIXED_POLARS_RUNTIME_FREE_THREADED_ABI'}}))
    with pytest.raises(AssertionError):
        release_artifacts.test_totals(report, 'macos-arm64', capabilities)
    capabilities.write_text(json.dumps({'platform': 'macos-arm64', 'free_threaded': True, 'unavailable': {}}))
    write_skips(report, (), 'tests.test_public_connection', UNIX_REASON)
    with pytest.raises(AssertionError):
        release_artifacts.test_totals(report, 'macos-arm64', capabilities)


def test_joint_provider_observation_gap_is_limited_to_its_five_real_cases(tmp_path: Path) -> None:
    from xml.etree import ElementTree
    report = tmp_path / 'tests.xml'
    write_skips(report, PARALLEL_NAMES, 'tests.test_benchmark_parallel', PARALLEL_REASON)
    tree = ElementTree.parse(report)
    root = tree.getroot()
    for mode in ('success', 'early-schema-failure', 'repeat-consumption',
                 'cached-dictionary-success', 'cached-dictionary-encoding-failure'):
        case = ElementTree.SubElement(root, 'testcase', classname='tests.test_polars_input',
            name=f'test_c_array_consumption_owns_both_releases_on_every_exit[{mode}]')
        ElementTree.SubElement(case, 'skipped', message='C Array release observation needs both optional providers')
    root.set('tests', '29')
    root.set('skipped', '29')
    tree.write(report)
    capabilities = tmp_path / 'providers.json'
    capabilities.write_text(json.dumps({'platform': 'windows-arm64', 'free_threaded': False,
        'unavailable': {'pyarrow': 'NO_FIXED_PROVIDER_WHEEL_WINDOWS_ARM64'}}))
    assert release_artifacts.test_totals(report, 'windows-arm64', capabilities)['skipped'] == 29
    root.findall('testcase')[-1].set('name', 'test_unrelated_polars_behavior')
    tree.write(report)
    with pytest.raises(AssertionError):
        release_artifacts.test_totals(report, 'windows-arm64', capabilities)


def feature_report(
    tmp_path: Path, target: str, free: bool,
    cases: list[tuple[str, str, str]], unavailable: dict[str, str] | None = None,
) -> tuple[Path, Path]:
    """Add exact new cases to the independently frozen baseline observations."""
    from xml.etree import ElementTree
    report = tmp_path / 'features.xml'
    if target.startswith('windows-'):
        write_skips(report, PARALLEL_NAMES, 'tests.test_benchmark_parallel', PARALLEL_REASON)
    else:
        write_skips(report, (UNIX_NAME,), 'tests.test_public_connection', UNIX_REASON)
    tree = ElementTree.parse(report)
    suite = tree.getroot()
    for classname, name, reason in cases:
        case = ElementTree.SubElement(suite, 'testcase', classname=classname, name=name)
        ElementTree.SubElement(case, 'skipped', message=reason)
    count = len(suite.findall('testcase'))
    suite.set('tests', str(count))
    suite.set('skipped', str(count))
    tree.write(report)
    capabilities = tmp_path / 'providers.json'
    capabilities.write_text(json.dumps({'platform': target, 'free_threaded': free,
                                        'unavailable': unavailable or {}}))
    return report, capabilities


@pytest.mark.parametrize('free', [False, True])
def test_nogil_observations_are_na_only_for_a_confirmed_ordinary_abi(tmp_path: Path, free: bool) -> None:
    names = ['test_native_identity_retains_a_disabled_gil']
    names += [f'test_nogil_real_materialization_and_ownership_overlap[{mode}-{label}]'
              for mode in ('atomic', 'busy', 'independent', 'closed-complete', 'closed-incomplete')
              for label in ('84', '97')]
    names += [f'test_nogil_cross_batch_dict_keys_release_native_cache_and_keep_delivered_owners[{label}]'
              for label in ('84', '97')]
    report, capabilities = feature_report(tmp_path, 'macos-arm64', free,
        [('tests.test_streaming_nogil', name, 'requires a verified GIL-disabled package') for name in names])
    if free:
        with pytest.raises(AssertionError, match='Unexpected skipped requirement'):
            release_artifacts.test_totals(report, 'macos-arm64', capabilities)
    else:
        assert release_artifacts.test_totals(report, 'macos-arm64', capabilities)['skipped'] == 14


@pytest.mark.parametrize('free', [False, True])
@pytest.mark.parametrize('target', ['windows-x86_64', 'windows-arm64'])
def test_tls_unix_na_is_exact_on_both_windows_abis(tmp_path: Path, target: str, free: bool) -> None:
    names = [f'test_unix_tls_diagnostic_identity_follows_explicit_name_only[{mode}]'
             for mode in ('REQUIRED', 'VERIFY_CA', 'VERIFY_IDENTITY')]
    names.append('test_default_plain_unix_connection_retains_no_tls_snapshot')
    report, capabilities = feature_report(tmp_path, target, free,
        [('tests.test_tls_expansion', name, 'Unix socket transport is unavailable') for name in names])
    assert release_artifacts.test_totals(report, target, capabilities)['skipped'] == 28


@pytest.mark.parametrize(('classname', 'name', 'reason'), [
    ('tests.test_callproc_cursor', 'test_database_identifier_with_backtick_when_fixture_privilege_allows[84]', 'CREATE DATABASE unavailable: OperationalError code=1044'),
    ('tests.test_callproc_cursor', 'test_server_slot_values_are_cleared_and_names_reused_without_touching_user_vars[False-84]', 'performance_schema variable inspection unavailable: OperationalError code=1142'),
    ('tests.test_callproc_cursor', 'test_pure_in', 'pre-API installed build: callproc/output property not registered'),
    ('tests.test_streaming_nogil', 'test_unapproved_nogil_case', 'requires a verified GIL-disabled package'),
    ('tests.test_streaming_nogil', 'test_native_identity_retains_a_disabled_gil', 'requires a verified GIL-disabled package changed'),
    ('tests.test_callproc_concurrency', 'test_parallel_call', 'requires a verified GIL-disabled package'),
])
def test_feature_na_never_hides_permissions_missing_api_or_unrelated_concurrency(
    tmp_path: Path, classname: str, name: str, reason: str,
) -> None:
    report, capabilities = feature_report(tmp_path, 'windows-arm64', False, [(classname, name, reason)])
    with pytest.raises(AssertionError, match='Unexpected skipped requirement'):
        release_artifacts.test_totals(report, 'windows-arm64', capabilities)


@pytest.mark.parametrize('missing', [False, True])
def test_new_arrow_cases_require_the_existing_windows_arm64_provider_declaration(tmp_path: Path, missing: bool) -> None:
    cases = [
        ('tests.test_feature_expansion_a_planner', 'test_arrow_natural_table_keeps_the_shared_row_role'),
        ('tests.test_feature_expansion_a_planner', 'test_nested_arrow_cells_publish_actual_writer_roles'),
        ('tests.test_feature_expansion_a_planner', 'test_prepared_display_rejects_nested_arrow_parameters'),
        ('tests.test_feature_expansion_a_escape', 'test_arrow_natural_table_projection_uses_shared_shapes'),
        ('tests.test_feature_expansion_a', 'test_native_rows_consume_provider_columns_directly[pyarrow]'),
        ('tests.test_feature_expansion_a', 'test_native_rows_keep_dictionary_labels_as_values_and_reject_nested_cells'),
        ('tests.test_feature_expansion_a', 'test_native_provider_text_and_binary_keep_bounded_fragments_and_error_offsets[pyarrow]'),
    ]
    unavailable = {'pyarrow': 'NO_FIXED_PROVIDER_WHEEL_WINDOWS_ARM64'} if missing else {}
    report, capabilities = feature_report(tmp_path, 'windows-arm64', False,
        [(module, name, "could not import 'pyarrow': No module named 'pyarrow'") for module, name in cases], unavailable)
    if missing:
        assert release_artifacts.test_totals(report, 'windows-arm64', capabilities)['skipped'] == 31
    else:
        with pytest.raises(AssertionError, match='Unexpected skipped requirement'):
            release_artifacts.test_totals(report, 'windows-arm64', capabilities)


def test_feature_na_needs_its_platform_and_real_abi_evidence(tmp_path: Path) -> None:
    case = ('tests.test_tls_expansion', 'test_default_plain_unix_connection_retains_no_tls_snapshot',
            'Unix socket transport is unavailable')
    report, capabilities = feature_report(tmp_path, 'macos-arm64', False, [case])
    with pytest.raises(AssertionError, match='Unexpected skipped requirement'):
        release_artifacts.test_totals(report, 'macos-arm64', capabilities)
    report, _ = feature_report(tmp_path, 'windows-arm64', False, [case])
    with pytest.raises(AssertionError, match='Unexpected skipped requirement'):
        release_artifacts.test_totals(report, 'windows-arm64')


def test_new_polars_case_is_na_only_for_the_declared_free_threaded_gap(tmp_path: Path) -> None:
    case = ('tests.test_feature_expansion_a', 'test_native_rows_consume_provider_columns_directly[polars]',
            "could not import 'polars': No module named 'polars'")
    report, capabilities = feature_report(tmp_path, 'macos-arm64', True, [case],
        {'polars': 'NO_FIXED_POLARS_RUNTIME_FREE_THREADED_ABI'})
    assert release_artifacts.test_totals(report, 'macos-arm64', capabilities)['skipped'] == 2
    report, capabilities = feature_report(tmp_path, 'macos-arm64', False, [case])
    with pytest.raises(AssertionError, match='Unexpected skipped requirement'):
        release_artifacts.test_totals(report, 'macos-arm64', capabilities)


@pytest.mark.parametrize('free', [False, True])
@pytest.mark.parametrize('target', ['macos-arm64', 'windows-arm64', 'linux-glibc-x86_64'])
def test_resource_nogil_cases_are_na_only_for_a_confirmed_ordinary_abi(
    tmp_path: Path, target: str, free: bool,
) -> None:
    cases = [('tests.test_streaming_resource_combinations',
              f'test_nogil_paused_prepared_rows_keep_atomic_ownership_during_concurrent_gc[{label}]',
              'requires a verified GIL-disabled package') for label in ('84', '97')]
    report, capabilities = feature_report(tmp_path, target, free, cases)
    if free:
        with pytest.raises(AssertionError, match='Unexpected skipped requirement'):
            release_artifacts.test_totals(report, target, capabilities)
    else:
        baseline = len(PARALLEL_NAMES) if target.startswith('windows-') else 1
        assert release_artifacts.test_totals(report, target, capabilities)['skipped'] == baseline + 2


@pytest.mark.parametrize(('module', 'name', 'reason'), [
    ('tests.test_streaming_resource_combinations',
     'test_nogil_paused_prepared_rows_keep_atomic_ownership_during_concurrent_gc[84]',
     'requires a verified GIL-disabled package changed'),
    ('tests.test_streaming_resource_combinations',
     'test_nogil_paused_prepared_rows_keep_atomic_ownership_during_concurrent_gc[97]',
     'requires explicitly selected owned MySQL fixtures'),
    ('tests.test_streaming_resource_combinations',
     'test_nogil_paused_prepared_rows_keep_atomic_ownership_during_concurrent_gc[98]',
     'requires a verified GIL-disabled package'),
    ('tests.test_streaming_nogil',
     'test_nogil_paused_prepared_rows_keep_atomic_ownership_during_concurrent_gc[84]',
     'requires a verified GIL-disabled package'),
    ('tests.test_streaming_resource_combinations', 'test_unknown_resource_skip',
     'requires a verified GIL-disabled package'),
])
def test_resource_nogil_na_rejects_wrong_reason_module_or_unknown_case(
    tmp_path: Path, module: str, name: str, reason: str,
) -> None:
    report, capabilities = feature_report(tmp_path, 'macos-arm64', False, [(module, name, reason)])
    with pytest.raises(AssertionError, match='Unexpected skipped requirement'):
        release_artifacts.test_totals(report, 'macos-arm64', capabilities)


@pytest.mark.parametrize('invalid_free', [0, 1, None, 'False'])
def test_resource_nogil_na_requires_an_exact_boolean_abi_declaration(
    tmp_path: Path, invalid_free: object,
) -> None:
    case = ('tests.test_streaming_resource_combinations',
            'test_nogil_paused_prepared_rows_keep_atomic_ownership_during_concurrent_gc[84]',
            'requires a verified GIL-disabled package')
    report, capabilities = feature_report(tmp_path, 'macos-arm64', False, [case])
    payload = json.loads(capabilities.read_text())
    payload['free_threaded'] = invalid_free
    capabilities.write_text(json.dumps(payload))
    with pytest.raises(AssertionError):
        release_artifacts.test_totals(report, 'macos-arm64', capabilities)


@pytest.mark.parametrize(('provider', 'name'), [
    ('pyarrow', 'test_optional_aware_cells_reject_non_utc_hint_before_target_send[arrow_array-False-84]'),
    ('pyarrow', 'test_optional_all_null_timezone_columns_still_write_sql_null[arrow_batch-True-97]'),
    ('pyarrow', 'test_optional_aware_cells_reject_an_unconfirmed_system_timezone_hint[arrow_table-True-84]'),
    ('pyarrow', 'test_optional_naive_or_unrelated_hint_does_not_create_a_timezone_refusal[aware_non_timezone-pandas_arrow-False-97]'),
    ('pyarrow', 'test_optional_naive_or_unrelated_hint_does_not_create_a_timezone_refusal[naive_non_utc-arrow_table-True-84]'),
    ('polars', 'test_optional_aware_cells_reject_non_utc_hint_before_target_send[polars_series-False-84]'),
    ('polars', 'test_optional_all_null_timezone_columns_still_write_sql_null[polars_table-True-97]'),
    ('polars', 'test_optional_naive_or_unrelated_hint_does_not_create_a_timezone_refusal[naive_non_utc-polars_table-True-84]'),
])
@pytest.mark.parametrize('declared', [False, True])
def test_optional_time_context_na_requires_the_real_provider_gap(
    tmp_path: Path, provider: str, name: str, declared: bool,
) -> None:
    target = 'windows-arm64' if provider == 'pyarrow' else 'macos-arm64'
    free = provider == 'polars'
    gap = ('NO_FIXED_PROVIDER_WHEEL_WINDOWS_ARM64' if provider == 'pyarrow'
           else 'NO_FIXED_POLARS_RUNTIME_FREE_THREADED_ABI')
    reason = f"could not import '{provider}': No module named '{provider}'"
    report, capabilities = feature_report(tmp_path, target, free,
        [('tests.test_binding_time_context_optional', name, reason)],
        {provider: gap} if declared else {})
    if declared:
        expected = len(PARALLEL_NAMES) + 1 if target.startswith('windows-') else 2
        assert release_artifacts.test_totals(report, target, capabilities)['skipped'] == expected
    else:
        with pytest.raises(AssertionError, match='Unexpected skipped requirement'):
            release_artifacts.test_totals(report, target, capabilities)


@pytest.mark.parametrize(('module', 'name', 'reason'), [
    ('tests.test_binding_time_context_optional',
     'test_optional_all_null_timezone_columns_still_write_sql_null[arrow_table-False-84]',
     'pre-API installed build: UTC guard unavailable'),
    ('tests.test_binding_time_context_optional',
     'test_optional_aware_cells_reject_non_utc_hint_before_target_send[arrow_scalar-True-84]',
     "could not import 'pyarrow': No module named 'pyarrow'"),
    ('tests.test_other_optional_module',
     'test_optional_aware_cells_reject_non_utc_hint_before_target_send[arrow_array-False-84]',
     "could not import 'pyarrow': No module named 'pyarrow'"),
    ('tests.test_binding_time_context_optional',
     'test_optional_all_null_timezone_columns_still_write_sql_null[arrow_table-False-84]',
     "could not import 'polars': No module named 'polars'"),
])
def test_optional_time_context_na_rejects_unknown_cases_or_non_import_failures(
    tmp_path: Path, module: str, name: str, reason: str,
) -> None:
    report, capabilities = feature_report(tmp_path, 'windows-arm64', False, [(module, name, reason)],
        {'pyarrow': 'NO_FIXED_PROVIDER_WHEEL_WINDOWS_ARM64'})
    with pytest.raises(AssertionError, match='Unexpected skipped requirement'):
        release_artifacts.test_totals(report, 'windows-arm64', capabilities)


def test_optional_time_context_polars_gap_cannot_apply_to_an_ordinary_abi(tmp_path: Path) -> None:
    case = ('tests.test_binding_time_context_optional',
            'test_optional_aware_cells_reject_non_utc_hint_before_target_send[polars_table-False-84]',
            "could not import 'polars': No module named 'polars'")
    report, capabilities = feature_report(tmp_path, 'macos-arm64', False, [case],
        {'polars': 'NO_FIXED_POLARS_RUNTIME_FREE_THREADED_ABI'})
    with pytest.raises(AssertionError):
        release_artifacts.test_totals(report, 'macos-arm64', capabilities)


@pytest.mark.parametrize(('mkfifo', 'af_unix'), [(False, False), (False, True), (True, False), (True, True)])
def test_provider_report_records_the_two_actual_runtime_primitives(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, mkfifo: bool, af_unix: bool,
) -> None:
    import sysconfig
    from types import SimpleNamespace

    identity = tmp_path / 'identity.json'
    identity.write_text(json.dumps({'platform': 'windows-arm64',
        'free_threaded': bool(sysconfig.get_config_var('Py_GIL_DISABLED'))}))
    output = tmp_path / 'providers.json'
    versions = {'numpy': '2.5.4', 'pandas': '3.1.0', 'pyarrow': '26.0.0', 'polars': '2.1.0'}

    def install(command: list[str], *, check: bool) -> None:
        assert check
        Path(command[command.index('--report') + 1]).write_text('{"install": []}')

    def provider(name: str) -> SimpleNamespace:
        return SimpleNamespace(__version__=versions[name])

    def absent(_name: str) -> None:
        return None

    # The package install/import is outside this metadata unit. Simulate only
    # its existing successful handoff; no network or optional module is loaded.
    monkeypatch.setattr(release_artifacts, 'subprocess', SimpleNamespace(run=install))
    monkeypatch.setattr(release_artifacts, 'import_module', provider)
    monkeypatch.setattr('importlib.util.find_spec', absent)
    monkeypatch.setattr(release_artifacts, 'os', SimpleNamespace(mkfifo=None) if mkfifo else SimpleNamespace())
    monkeypatch.setattr(release_artifacts, 'socket', SimpleNamespace(AF_UNIX=None) if af_unix else SimpleNamespace())
    release_artifacts.prepare_providers(identity, output)
    recorded = json.loads(output.read_text())
    assert recorded['has_os_mkfifo'] is mkfifo
    assert recorded['has_socket_af_unix'] is af_unix


@pytest.mark.parametrize('target', ['windows-x86_64', 'windows-arm64'])
@pytest.mark.parametrize('free', [False, True])
@pytest.mark.parametrize('missing', ['fifo', 'socket', 'both'])
def test_windows_ca_primitive_na_requires_independent_missing_runtime_facts(
    tmp_path: Path, target: str, free: bool, missing: str,
) -> None:
    cases = [
        ('tests.test_tls_material_boundaries',
         f'test_special_ca_object_is_rejected_without_waiting_for_stream_contents[{kind}-{source}]',
         f'CPython does not expose {primitive}')
        for kind, primitive in [('fifo', 'os.mkfifo'), ('socket', 'socket.AF_UNIX')]
        for source in ('file', 'dir')
        if missing in (kind, 'both')
    ]
    report, capabilities = feature_report(tmp_path, target, free, cases)
    payload = json.loads(capabilities.read_text())
    payload.update(has_os_mkfifo=missing not in ('fifo', 'both'),
                   has_socket_af_unix=missing not in ('socket', 'both'))
    capabilities.write_text(json.dumps(payload))
    assert release_artifacts.test_totals(report, target, capabilities)['skipped'] == len(PARALLEL_NAMES) + len(cases)


@pytest.mark.parametrize(('kind', 'primitive', 'field'), [
    ('fifo', 'os.mkfifo', 'has_os_mkfifo'), ('socket', 'socket.AF_UNIX', 'has_socket_af_unix'),
])
@pytest.mark.parametrize('observed', [True, 0, None, 'False'])
def test_ca_primitive_na_needs_its_own_exact_false_runtime_fact(
    tmp_path: Path, kind: str, primitive: str, field: str, observed: object,
) -> None:
    case = ('tests.test_tls_material_boundaries',
        f'test_special_ca_object_is_rejected_without_waiting_for_stream_contents[{kind}-file]',
        f'CPython does not expose {primitive}')
    report, capabilities = feature_report(tmp_path, 'windows-arm64', False, [case])
    payload = json.loads(capabilities.read_text())
    payload.update(has_os_mkfifo=False, has_socket_af_unix=False)
    payload[field] = observed
    capabilities.write_text(json.dumps(payload))
    with pytest.raises(AssertionError, match='Unexpected skipped requirement'):
        release_artifacts.test_totals(report, 'windows-arm64', capabilities)


@pytest.mark.parametrize(('module', 'name', 'reason'), [
    ('tests.test_tls_expansion',
     'test_special_ca_object_is_rejected_without_waiting_for_stream_contents[fifo-file]',
     'CPython does not expose os.mkfifo'),
    ('tests.test_tls_material_boundaries',
     'test_special_ca_object_is_rejected_without_waiting_for_stream_contents[fifo-unknown]',
     'CPython does not expose os.mkfifo'),
    ('tests.test_tls_material_boundaries',
     'test_special_ca_object_is_rejected_without_waiting_for_stream_contents[socket-file]',
     'CPython does not expose os.mkfifo'),
    ('tests.test_tls_material_boundaries',
     'test_special_ca_object_is_rejected_without_waiting_for_stream_contents[fifo-file]',
     'CPython does not expose socket.AF_UNIX'),
    ('tests.test_tls_material_boundaries',
     'test_special_ca_object_is_rejected_without_waiting_for_stream_contents[fifo-dir]',
     'requires POSIX FIFO and Unix socket file objects'),
    ('tests.test_tls_material_boundaries',
     'test_actual_unreadable_ca_preserves_permission_error_and_member_path[file]',
     'root cannot establish this test account permission boundary'),
])
def test_ca_primitive_na_rejects_wrong_nodes_reasons_and_permission_skips(
    tmp_path: Path, module: str, name: str, reason: str,
) -> None:
    report, capabilities = feature_report(tmp_path, 'windows-arm64', False, [(module, name, reason)])
    payload = json.loads(capabilities.read_text())
    payload.update(has_os_mkfifo=False, has_socket_af_unix=False)
    capabilities.write_text(json.dumps(payload))
    with pytest.raises(AssertionError, match='Unexpected skipped requirement'):
        release_artifacts.test_totals(report, 'windows-arm64', capabilities)


@pytest.mark.parametrize('target', ['macos-arm64', 'linux-glibc-x86_64', 'linux-musl-aarch64'])
@pytest.mark.parametrize(('kind', 'primitive'), [('fifo', 'os.mkfifo'), ('socket', 'socket.AF_UNIX')])
def test_ca_primitive_na_never_applies_to_non_windows_targets(
    tmp_path: Path, target: str, kind: str, primitive: str,
) -> None:
    case = ('tests.test_tls_material_boundaries',
        f'test_special_ca_object_is_rejected_without_waiting_for_stream_contents[{kind}-file]',
        f'CPython does not expose {primitive}')
    report, capabilities = feature_report(tmp_path, target, False, [case])
    payload = json.loads(capabilities.read_text())
    payload.update(has_os_mkfifo=False, has_socket_af_unix=False)
    capabilities.write_text(json.dumps(payload))
    with pytest.raises(AssertionError, match='Unexpected skipped requirement'):
        release_artifacts.test_totals(report, target, capabilities)


def test_ca_primitive_na_requires_an_actual_runtime_report(tmp_path: Path) -> None:
    case = ('tests.test_tls_material_boundaries',
        'test_special_ca_object_is_rejected_without_waiting_for_stream_contents[fifo-file]',
        'CPython does not expose os.mkfifo')
    report, _ = feature_report(tmp_path, 'windows-arm64', False, [case])
    with pytest.raises(AssertionError, match='Unexpected skipped requirement'):
        release_artifacts.test_totals(report, 'windows-arm64')


@pytest.mark.parametrize('damage', ['missing', 'changed', 'undeclared'])
def test_actual_wheel_members_cannot_omit_change_or_hide_license_material(
    tmp_path: Path, synthetic_license_materials: dict[str, bytes], damage: str,
) -> None:
    version = release_artifacts.release_version()
    wheel = tmp_path / f'sqlaero-{version}-cp313-cp313-macosx_11_0_arm64.whl'
    missing = 'licenses/third-party/example-1.0.0/LICENSE'
    with ZipFile(wheel, 'w') as archive:
        archive.writestr('sqlaero/_sqlaero.so', b'non-executable format fixture')
        archive.writestr(f'sqlaero-{version}.dist-info/METADATA',
            f'Metadata-Version: 2.4\nName: sqlaero\nVersion: {version}\n'
            + ''.join(f'License-File: {name}\n' for name in synthetic_license_materials
                      if not (damage == 'undeclared' and name == missing)))
        for name, raw in synthetic_license_materials.items():
            if damage == 'missing' and name == missing:
                continue
            archive.writestr(f'sqlaero-{version}.dist-info/licenses/{name}',
                             b'wrong original text' if damage == 'changed' and name == missing else raw)
    with pytest.raises((AssertionError, KeyError)):
        release_artifacts.wheel_record(wheel)


@pytest.mark.parametrize('damage', ['none', 'missing', 'changed'])
def test_canonical_sdist_contains_the_same_original_license_bytes(
    tmp_path: Path, synthetic_license_materials: dict[str, bytes], damage: str,
) -> None:
    version = release_artifacts.release_version()
    package = tmp_path / f'sqlaero-{version}.tar.gz'
    missing = 'licenses/third-party/example-1.0.0/LICENSE'
    with tarfile.open(package, 'w:gz') as archive:
        for name, raw in synthetic_license_materials.items():
            if damage == 'missing' and name == missing:
                continue
            payload = b'changed copyright' if damage == 'changed' and name == missing else raw
            member = tarfile.TarInfo(f'sqlaero-{version}/{name}')
            member.size = len(payload)
            archive.addfile(member, io.BytesIO(payload))
    if damage == 'none':
        assert release_artifacts.sdist_record(package)['license_file_count'] == 3
    else:
        with pytest.raises((AssertionError, KeyError)):
            release_artifacts.sdist_record(package)


@pytest.mark.parametrize('damage', ['none', 'missing-entry', 'missing-text'])
def test_current_production_graph_catches_old_as_well_as_new_notice_omissions(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, damage: str,
) -> None:
    """The prior zerocopy-derive omission must fail without a dependency diff."""
    (tmp_path / 'LICENSE').write_text('project license\n')
    (tmp_path / 'Cargo.toml').write_text('[workspace.package]\nversion="0.2.0-beta.4"\n')
    (tmp_path / 'Cargo.lock').write_text('[[package]]\nname="zerocopy-derive"\nversion="0.8.63"\n')
    notices = ('| Package | Version | Declared license | Original texts |\n'
               '| --- | --- | --- | --- |\n'
               '| example | 1.0.0 | MIT | [LICENSE](licenses/third-party/example-1.0.0/LICENSE) |\n')
    if damage != 'missing-entry':
        notices += ('| zerocopy-derive | 0.8.63 | MIT | '
                    '[LICENSE](licenses/third-party/zerocopy-derive-0.8.63/LICENSE) |\n')
    (tmp_path / 'THIRD_PARTY_NOTICES.md').write_text(notices)
    for name in ('example-1.0.0', 'zerocopy-derive-0.8.63'):
        directory = tmp_path / 'licenses/third-party' / name
        directory.mkdir(parents=True)
        if not (damage == 'missing-text' and name.startswith('zerocopy')):
            (directory / 'LICENSE').write_text('original copyright fixture\n')
    (tmp_path / 'pyproject.toml').write_text('[project]\nversion="0.2.0b4"\n')
    monkeypatch.setattr(release_artifacts, 'ROOT', tmp_path)

    def graph(command: list[str], **kwargs: object) -> subprocess.CompletedProcess[str]:
        assert command[1:3] == ['tree', '--locked']
        assert command[command.index('--edges') + 1] == 'normal,build'
        return subprocess.CompletedProcess(command, 0,
            'sqlaero-python v0.2.0-beta.4 (/source)\nzerocopy-derive v0.8.63\n', '')

    monkeypatch.setattr(release_artifacts.subprocess, 'run', graph)
    output = tmp_path / 'notices.json'
    if damage == 'none':
        release_artifacts.source_notices(['aarch64-apple-darwin'], output)
        assert json.loads(output.read_text())['union_packages'] == 1
    else:
        with pytest.raises(AssertionError, match='Missing'):
            release_artifacts.source_notices(['aarch64-apple-darwin'], output)
        assert not output.exists()


@pytest.mark.parametrize(('provider', 'function', 'source'), [
    ('pyarrow', 'test_out_requires_literal_none_before_target_send', 'arrow-null'),
    ('polars', 'test_out_requires_literal_none_before_target_send', 'polars-null'),
    ('polars', 'test_literal_none_out_survives_shared_source_preparation', 'polars-object'),
])
@pytest.mark.parametrize('declared', [False, True])
def test_out_source_cases_can_skip_only_for_the_exact_known_provider_gap(
    tmp_path: Path, provider: str, function: str, source: str, declared: bool,
) -> None:
    target = 'windows-arm64' if provider == 'pyarrow' else 'macos-arm64'
    free = provider == 'polars'
    gap = ('NO_FIXED_PROVIDER_WHEEL_WINDOWS_ARM64' if provider == 'pyarrow'
           else 'NO_FIXED_POLARS_RUNTIME_FREE_THREADED_ABI')
    cases = [('tests.test_callproc_cursor', f'{function}[{label}-{source}-{prepared}-{kind}]',
              f"could not import '{provider}': No module named '{provider}'")
             for label in ('84', '97') for prepared in (False, True)
             for kind in ('Cursor', 'DictCursor', 'SSCursor', 'SSDictCursor')]
    report, capabilities = feature_report(tmp_path, target, free, cases,
                                         {provider: gap} if declared else {})
    if declared:
        baseline = len(PARALLEL_NAMES) if target.startswith('windows-') else 1
        assert release_artifacts.test_totals(report, target, capabilities)['skipped'] == baseline + 16
    else:
        with pytest.raises(AssertionError, match='Unexpected skipped requirement'):
            release_artifacts.test_totals(report, target, capabilities)
