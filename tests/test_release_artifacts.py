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

    monkeypatch.setattr(release_artifacts, 'wheel_record', wheel_record)
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
