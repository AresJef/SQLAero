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
from importlib.metadata import distribution, version
import json
import os
from pathlib import Path
import platform
import re
import socket
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
RUST_TARGETS = (
    'aarch64-apple-darwin', 'x86_64-apple-darwin',
    'x86_64-pc-windows-msvc', 'aarch64-pc-windows-msvc',
    'x86_64-unknown-linux-gnu', 'aarch64-unknown-linux-gnu',
    'x86_64-unknown-linux-musl', 'aarch64-unknown-linux-musl',
)
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
    'test_parallel_binds_each_designated_input_and_keeps_shared_default',
    'test_parallel_factory_inputs_are_private_reused_and_checked_for_mutation',
    'test_cold_first_observation_precedes_checks_and_default_stays_hot_only',
    'test_cold_first_guards_and_original_creation_error_are_preserved',
})
NOGIL_ONLY_NA = frozenset({
    'test_native_identity_retains_a_disabled_gil',
    *(f'test_nogil_real_materialization_and_ownership_overlap[{case}-{label}]'
      for case in ('atomic', 'busy', 'independent', 'closed-complete', 'closed-incomplete')
      for label in ('84', '97')),
    *(f'test_nogil_cross_batch_dict_keys_release_native_cache_and_keep_delivered_owners[{label}]'
      for label in ('84', '97')),
})
RESOURCE_NOGIL_ONLY_NA = frozenset({
    ('tests.test_streaming_resource_combinations',
     f'test_nogil_paused_prepared_rows_keep_atomic_ownership_during_concurrent_gc[{label}]')
    for label in ('84', '97')
})
WINDOWS_TLS_UNIX_NA = frozenset({
    *(f'test_unix_tls_diagnostic_identity_follows_explicit_name_only[{mode}]'
      for mode in ('REQUIRED', 'VERIFY_CA', 'VERIFY_IDENTITY')),
    'test_default_plain_unix_connection_retains_no_tls_snapshot',
})
FEATURE_PROVIDER_NA = {
    'pyarrow': frozenset({
        ('tests.test_feature_expansion_a_planner', 'test_arrow_natural_table_keeps_the_shared_row_role'),
        ('tests.test_feature_expansion_a_planner', 'test_nested_arrow_cells_publish_actual_writer_roles'),
        ('tests.test_feature_expansion_a_planner', 'test_prepared_display_rejects_nested_arrow_parameters'),
        ('tests.test_feature_expansion_a_escape', 'test_arrow_natural_table_projection_uses_shared_shapes'),
        ('tests.test_feature_expansion_a', 'test_native_rows_consume_provider_columns_directly[pyarrow]'),
        ('tests.test_feature_expansion_a', 'test_native_rows_keep_dictionary_labels_as_values_and_reject_nested_cells'),
        ('tests.test_feature_expansion_a', 'test_native_provider_text_and_binary_keep_bounded_fragments_and_error_offsets[pyarrow]'),
        *(('tests.test_binding_time_context_optional', f'{function}[{kind}-{prepared}-{label}]')
          for function in ('test_optional_aware_cells_reject_non_utc_hint_before_target_send',
                           'test_optional_all_null_timezone_columns_still_write_sql_null')
          for kind in ('pandas_arrow', 'arrow_array', 'arrow_batch', 'arrow_table')
          for prepared in (False, True) for label in ('84', '97')),
        *(('tests.test_binding_time_context_optional',
           f'test_optional_aware_cells_reject_an_unconfirmed_system_timezone_hint[arrow_table-{prepared}-{label}]')
          for prepared in (False, True) for label in ('84', '97')),
        *(('tests.test_binding_time_context_optional',
           f'test_optional_naive_or_unrelated_hint_does_not_create_a_timezone_refusal[{scenario}-{kind}-{prepared}-{label}]')
          for scenario in ('naive_non_utc', 'aware_non_timezone') for kind in ('pandas_arrow', 'arrow_table')
          for prepared in (False, True) for label in ('84', '97')),
        *(('tests.test_callproc_cursor',
           f'test_out_requires_literal_none_before_target_send[{label}-arrow-null-{prepared}-{kind}]')
          for label in ('84', '97') for prepared in (False, True)
          for kind in ('Cursor', 'DictCursor', 'SSCursor', 'SSDictCursor')),
    }),
    'polars': frozenset({
        ('tests.test_feature_expansion_a', 'test_native_rows_consume_provider_columns_directly[polars]'),
        *(('tests.test_binding_time_context_optional', f'{function}[{kind}-{prepared}-{label}]')
          for function in ('test_optional_aware_cells_reject_non_utc_hint_before_target_send',
                           'test_optional_all_null_timezone_columns_still_write_sql_null')
          for kind in ('polars_series', 'polars_table')
          for prepared in (False, True) for label in ('84', '97')),
        *(('tests.test_binding_time_context_optional',
           f'test_optional_naive_or_unrelated_hint_does_not_create_a_timezone_refusal[{scenario}-polars_table-{prepared}-{label}]')
          for scenario in ('naive_non_utc', 'aware_non_timezone')
          for prepared in (False, True) for label in ('84', '97')),
        *(('tests.test_callproc_cursor', f'{function}[{label}-{source}-{prepared}-{kind}]')
          for function, source in (
              ('test_out_requires_literal_none_before_target_send', 'polars-null'),
              ('test_literal_none_out_survives_shared_source_preparation', 'polars-object'))
          for label in ('84', '97') for prepared in (False, True)
          for kind in ('Cursor', 'DictCursor', 'SSCursor', 'SSDictCursor')),
    }),
}


def digest(path: Path) -> str:
    """Hash file contents without loading a distribution into Python memory."""
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def release_version() -> str:
    """Read the sole Python distribution version from the frozen checkout."""
    return str(tomllib.loads((ROOT / 'pyproject.toml').read_text(encoding='utf-8'))['project']['version'])


def license_materials() -> dict[str, str]:
    """Hash actual source materials and reject broken or empty notice links."""
    notices = ROOT / 'THIRD_PARTY_NOTICES.md'
    links = re.findall(r'\]\((licenses/third-party/[^)]+)\)', notices.read_text(encoding='utf-8'))
    assert links, 'The third-party inventory has no original-text links'
    for name in links:
        path = ROOT / name
        assert path.resolve().is_relative_to((ROOT / 'licenses/third-party').resolve()), name
        assert path.is_file() and path.stat().st_size > 0, ('Missing license material', name)
    paths = [ROOT / 'LICENSE', notices,
             *sorted((ROOT / 'licenses/third-party').rglob('*'))]
    result: dict[str, str] = {}
    for path in paths:
        assert not path.is_symlink(), ('License material must be an owned file', path)
        if path.is_file():
            assert path.stat().st_size > 0, path
            result[path.relative_to(ROOT).as_posix()] = digest(path)
    return result


def source_notices(targets: list[str], output: Path) -> None:
    """Check the actual locked production graphs against the notice inventory.

    cargo tree normal/build edges use each target's real feature selection.
    Unfiltered metadata and a diff of newly added packages cannot prove this.
    Existing extra-target notices are deliberately retained.
    """
    materials = license_materials()
    inventory: dict[tuple[str, str], tuple[str, str]] = {}
    for line in (ROOT / 'THIRD_PARTY_NOTICES.md').read_text(encoding='utf-8').splitlines():
        fields = [field.strip() for field in line.split('|')]
        if len(fields) == 6 and fields[1] not in ('Package', '---'):
            key = fields[1], fields[2]
            assert key not in inventory, ('Duplicate third-party entry', key)
            inventory[key] = fields[3], fields[4]
    locked = {(item['name'], item['version']): item
              for item in tomllib.loads((ROOT / 'Cargo.lock').read_text(encoding='utf-8'))['package']}
    rust_version = str(tomllib.loads((ROOT / 'Cargo.toml').read_text(encoding='utf-8'))
                       ['workspace']['package']['version'])
    for path in (ROOT / 'licenses/third-party').glob('*/SOURCE.json'):
        source = json.loads(path.read_text(encoding='utf-8'))
        key = source['package'], source['version']
        assert source['crate_sha256'] == locked[key]['checksum'], ('Crate provenance mismatch', key)
        assert inventory[key][0] == source['declared_license'], ('License expression mismatch', key)
        for material in source['materials']:
            original = path.parent / material['path']
            assert original.resolve().is_relative_to(path.parent.resolve()), original
            assert digest(original) == material['sha256'], ('Original text changed', original)
    graphs: dict[str, list[list[str]]] = {}
    union: set[tuple[str, str]] = set()
    for target in targets:
        assert target in RUST_TARGETS, target
        process = subprocess.run([
            os.environ.get('CARGO', 'cargo'), 'tree', '--locked',
            '-p', 'sqlaero-python', '--target', target, '--edges', 'normal,build',
            '--prefix', 'none', '--format', '{p}',
        ], cwd=ROOT, check=True, capture_output=True, text=True)
        packages = {(match[1], match[2]) for line in process.stdout.splitlines()
                    if (match := re.match(r'^([\w-]+) v([^\s]+)', line)) is not None}
        assert ('sqlaero-python', rust_version) in packages, ('No production graph', target)
        packages -= {('sqlaero-python', rust_version), ('sqlaero-core', rust_version)}
        assert packages, target
        for key in packages:
            assert key in locked, ('Graph package is not locked', target, key)
            assert key in inventory, ('Missing third-party inventory entry', target, key)
            prefix = f'licenses/third-party/{key[0]}-{key[1]}/'
            links = re.findall(r'\]\(([^)]+)\)', inventory[key][1])
            originals = [name for name in links if name.startswith(prefix) and not name.endswith('/SOURCE.json')]
            assert originals and all(name in materials for name in originals), (
                'Missing original license texts', target, key)
        union.update(packages)
        graphs[target] = [list(key) for key in sorted(packages)]
    result = {'cargo_lock_sha256': digest(ROOT / 'Cargo.lock'),
              'graphs': graphs, 'union_packages': len(union), 'license_files': materials}
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
    print(f'Validated {len(union)} production dependencies across {len(graphs)} targets')


def sdist_record(path: Path) -> dict[str, Any]:
    """Read original license bytes from the actual canonical source archive."""
    expected = license_materials()
    with tarfile.open(path) as archive:
        prefix = f'sqlaero-{release_version()}/'
        for name, sha in expected.items():
            stream = archive.extractfile(prefix + name)
            assert stream is not None, ('Missing sdist license', name)
            with stream:
                hashed = hashlib.sha256()
                while chunk := stream.read(1024 * 1024):
                    hashed.update(chunk)
                assert hashed.hexdigest() == sha, (
                    'sdist license differs from source', name)
    return {'file': path.name, 'sha256': digest(path), 'bytes': path.stat().st_size,
            'license_file_count': len(expected)}


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
        materials = license_materials()
        declared = parsed.get_all('License-File', [])
        assert len(declared) == len(set(declared)) and set(declared) == set(materials), (
            'Wheel License-File declarations differ from source', path.name)
        prefix = metadata[0].rsplit('/', 1)[0] + '/licenses/'
        for name, sha in materials.items():
            assert hashlib.sha256(archive.read(prefix + name)).hexdigest() == sha, (
                'Wheel license differs from source', path.name, name)
        native_hash = hashlib.sha256(archive.read(native[0])).hexdigest()
    return {'file': path.name, 'sha256': digest(path), 'bytes': path.stat().st_size,
            'version': package_version, 'python': python, 'abi': abi, 'platform': target,
            'native_member': native[0], 'native_sha256': native_hash,
            'license_file_count': len(materials),
            'license_materials_sha256': hashlib.sha256(
                json.dumps(materials, sort_keys=True).encode('utf-8')).hexdigest()}


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
    package_files = distribution('sqlaero')
    metadata_file = next(file for file in package_files.files or ()
                         if str(file).endswith('.dist-info/METADATA'))
    license_root = Path(str(package_files.locate_file(metadata_file))).parent / 'licenses'
    for name, sha in license_materials().items():
        assert digest(license_root / name) == sha, ('Installed license differs from source', name)
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
    sdist_record(path)
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


def prepare_providers(identity: Path, output: Path) -> None:
    """Install the four frozen optional providers for this actual CI runtime.

    Known upstream wheel gaps remain explicit unavailable capabilities. An
    unexpected install/import failure is never downgraded to that state.
    """
    from importlib.util import find_spec
    record: dict[str, Any] = json.loads(identity.read_text(encoding='utf-8'))
    target = str(record['platform'])
    free = bool(sysconfig.get_config_var('Py_GIL_DISABLED'))
    assert target in PLATFORM_TAGS and free == bool(record['free_threaded'])
    gil = getattr(sys, '_is_gil_enabled', lambda: True)
    assert bool(gil()) is not free
    packages = {'numpy': '2.5.4', 'pandas': '3.1.0', 'pyarrow': '26.0.0', 'polars': '2.1.0'}
    unavailable: dict[str, str] = {}
    if target == 'windows-arm64':
        unavailable['pyarrow'] = 'NO_FIXED_PROVIDER_WHEEL_WINDOWS_ARM64'
    if free:
        unavailable['polars'] = 'NO_FIXED_POLARS_RUNTIME_FREE_THREADED_ABI'
    required = {name: selected for name, selected in packages.items() if name not in unavailable}
    output.parent.mkdir(parents=True, exist_ok=True)
    pip_report = output.with_suffix('.pip.json')
    subprocess.run([sys.executable, '-m', 'pip', 'install', '--disable-pip-version-check',
                    '--only-binary=:all:', '--report', str(pip_report),
                    *[f'{name}=={selected}' for name, selected in required.items()]], check=True)
    loaded: dict[str, Any] = {}
    for name, selected in required.items():
        before = bool(gil())
        module = import_module(name)
        actual = str(getattr(module, '__version__'))
        after = bool(gil())
        assert actual == selected and before is after is not free, (name, actual, before, after)
        loaded[name] = {'version': actual, 'gil_before': before, 'gil_after': after}
    for name in unavailable:
        assert find_spec(name) is None, ('Unexpected provider changes the frozen capability context', name)
    result = {'platform': target, 'free_threaded': free, 'packages': loaded,
              'unavailable': unavailable, 'pip_report_sha256': digest(pip_report),
              'has_os_mkfifo': hasattr(os, 'mkfifo'),
              'has_socket_af_unix': hasattr(socket, 'AF_UNIX')}
    output.write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(result))


def test_totals(path: Path, target: str | None = None, providers: Path | None = None) -> dict[str, int]:
    """Read the actual JUnit outcome rather than inferring tests from filenames."""
    root = ElementTree.parse(path).getroot()
    suites = [root] if root.tag == 'testsuite' else list(root.findall('testsuite'))
    totals = {name: sum(int(suite.get(name, '0')) for suite in suites)
              for name in ('tests', 'failures', 'errors', 'skipped')}
    assert totals['tests'] > 0 and totals['failures'] == totals['errors'] == 0, (path, totals)
    windows = target in ('windows-x86_64', 'windows-arm64')
    free: bool | None = None
    unavailable: dict[str, str] = {}
    mkfifo_absent = False
    af_unix_absent = False
    if providers is not None:
        capabilities: dict[str, Any] = json.loads(providers.read_text(encoding='utf-8'))
        assert target is None or capabilities['platform'] == target
        assert capabilities['platform'] in PLATFORMS
        assert type(capabilities['free_threaded']) is bool
        free = capabilities['free_threaded']
        unavailable = capabilities['unavailable']
        mkfifo_absent = capabilities.get('has_os_mkfifo') is False
        af_unix_absent = capabilities.get('has_socket_af_unix') is False
        assert set(unavailable) <= {'polars', 'pyarrow'}
        if 'polars' in unavailable:
            assert capabilities['free_threaded'] is True
            assert unavailable['polars'] == 'NO_FIXED_POLARS_RUNTIME_FREE_THREADED_ABI'
        if 'pyarrow' in unavailable:
            assert capabilities['platform'] == 'windows-arm64'
            assert unavailable['pyarrow'] == 'NO_FIXED_PROVIDER_WHEEL_WINDOWS_ARM64'
    baseline_skips = 0
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
            module_skip = (identity[0] == '' and skipped.get('message') == 'collection skipped'
                           and ((identity[1] == 'tests.test_polars_input' and 'polars' in unavailable)
                                or (identity[1] == 'tests.test_arrow_input' and 'pyarrow' in unavailable)))
            arrow_case = ('pyarrow' in unavailable
                          and identity[0] in ('tests.test_numpy_input', 'tests.test_pandas_input', 'tests.test_extended_input_mysql')
                          and skipped.get('message', '').startswith("could not import 'pyarrow':"))
            polars_case = ('polars' in unavailable
                           and identity[0] == 'tests.test_extended_input_mysql'
                           and skipped.get('message', '').startswith("could not import 'polars':"))
            joint_release = ('pyarrow' in unavailable
                             and identity[0] == 'tests.test_polars_input'
                             and identity[1] in {
                                 'test_c_array_consumption_owns_both_releases_on_every_exit[success]',
                                 'test_c_array_consumption_owns_both_releases_on_every_exit[early-schema-failure]',
                                 'test_c_array_consumption_owns_both_releases_on_every_exit[repeat-consumption]',
                                 'test_c_array_consumption_owns_both_releases_on_every_exit[cached-dictionary-success]',
                                 'test_c_array_consumption_owns_both_releases_on_every_exit[cached-dictionary-encoding-failure]',
                             }
                             and skipped.get('message') == 'C Array release observation needs both optional providers')
            nogil_only = (free is False
                          and ((identity[0] == 'tests.test_streaming_nogil' and identity[1] in NOGIL_ONLY_NA)
                               or identity in RESOURCE_NOGIL_ONLY_NA)
                          and skipped.get('message') == 'requires a verified GIL-disabled package')
            tls_unix = (free is not None and windows and identity[0] == 'tests.test_tls_expansion'
                        and identity[1] in WINDOWS_TLS_UNIX_NA
                        and skipped.get('message') == 'Unix socket transport is unavailable')
            # A real absent CPython primitive can exempt only its exact two
            # Windows source forms. Permissions and available primitives run.
            tls_primitive = (free is not None and windows
                and identity[0] == 'tests.test_tls_material_boundaries'
                and ((mkfifo_absent and identity[1] in {
                    'test_special_ca_object_is_rejected_without_waiting_for_stream_contents[fifo-file]',
                    'test_special_ca_object_is_rejected_without_waiting_for_stream_contents[fifo-dir]',
                } and skipped.get('message') == 'CPython does not expose os.mkfifo')
                or (af_unix_absent and identity[1] in {
                    'test_special_ca_object_is_rejected_without_waiting_for_stream_contents[socket-file]',
                    'test_special_ca_object_is_rejected_without_waiting_for_stream_contents[socket-dir]',
                } and skipped.get('message') == 'CPython does not expose socket.AF_UNIX')))
            feature_provider = any(
                provider in unavailable and identity in cases
                and skipped.get('message', '').startswith(f"could not import '{provider}':")
                for provider, cases in FEATURE_PROVIDER_NA.items()
            )
            baseline_skips += int(unix_socket or parallel)
            assert (unix_socket or parallel or module_skip or arrow_case or polars_case
                    or joint_release or nogil_only or tls_unix or tls_primitive or feature_provider), (
                        'Unexpected skipped requirement', path, case.attrib, skipped.attrib)
            assert identity not in seen, ('Duplicate skipped requirement', path, identity)
            seen.add(identity)
    assert len(seen) == totals['skipped'], ('Skipped total differs from cases', path, totals)
    if providers is not None:
        assert baseline_skips == (len(WINDOWS_PARALLEL_NA) if windows else 1), (path, baseline_skips)
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
    source_record = sdist_record(sdists[0])
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
            provider_path = path.with_name(f'{kind}-providers.json')
            assert not free_threaded or provider_path.is_file(), ('48-runtime validation requires actual provider evidence', provider_path)
            provider_evidence = provider_path if provider_path.is_file() else None
            totals = test_totals(path.with_name(f'python-{kind}.xml'), target=key[0], providers=provider_evidence)
            if provider_evidence is None:
                assert totals['skipped'] == (len(WINDOWS_PARALLEL_NA) if key[0].startswith('windows-') else 1), (path, totals)
            assert bool(record['free_threaded']) == key[2].endswith('t')
            assert bool(record['gil_enabled']) is not bool(record['free_threaded'])
            evidence.append({'kind': kind, 'platform': key[0], 'python': key[1], 'abi': key[2],
                             'wheel_sha256': record['sha256'],
                             'tests': totals, 'providers': json.loads(provider_path.read_text(encoding='utf-8')) if provider_evidence is not None else None})
    assert len(commits) == 1, commits
    result = {'version': release_version(), 'source_commit': next(iter(commits)),
              'wheels': wheels, 'sdist': source_record, 'evidence': evidence}
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
    provider = commands.add_parser('providers')
    provider.add_argument('--identity', type=Path, required=True)
    provider.add_argument('--output', type=Path, required=True)
    notices = commands.add_parser('notices')
    notices.add_argument('--target', action='append', choices=RUST_TARGETS)
    notices.add_argument('--output', type=Path, required=True)
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
    elif args.command == 'providers':
        prepare_providers(args.identity, args.output)
    elif args.command == 'notices':
        source_notices(args.target or list(RUST_TARGETS), args.output)
    elif args.command == 'extract':
        extract(args.sdist, args.destination, args.expected_sha)
    elif args.command == 'download':
        download(args.version, args.manifest, args.destination, args.output)
    else:
        manifest(args.directory, args.identities, args.output, args.free_threaded)


if __name__ == '__main__':
    main()
