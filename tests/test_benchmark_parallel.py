"""Check the fixed-work parallel entry without collecting performance evidence."""
from __future__ import annotations

import importlib.util
import io
import json
from argparse import Namespace
from pathlib import Path
import subprocess
import sys
import threading
import time
from typing import Any
from types import SimpleNamespace

import pytest

pytestmark = pytest.mark.skipif(
    sys.platform == "win32",
    reason="Parallel performance observations require POSIX resource snapshots and are measured on macOS",
)


@pytest.fixture
def runner() -> Any:
    path = Path(__file__).resolve().parents[1] / 'tools/benchmark.py'
    spec = importlib.util.spec_from_file_location('_parallel_boundary_benchmark', path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


@pytest.mark.parametrize('case_ids,extra', [
    (['escape/none/v0/scalar'], ['--threads', '3']),
    (['escape/none/v0/scalar'], ['--calls', '0']),
    (['escape/none/v0/scalar'], ['--calls', '-1']),
    (['escape/none/v0/scalar'], ['--calls', '1.5']),
    (['escape/none/v0/scalar'], ['--cycles', '0']),
    (['escape/none/v0/scalar'], ['--python-work', '-1']),
    (['escape/none/v0/scalar'], ['--seconds', 'nan']),
    (['escape/none/v0/scalar'], ['--seconds', 'inf']),
    (['escape/none/v0/scalar', 'escape/bool/v0/scalar'], []),
    (['escape/none/v0/scalar'], ['--prefix', 'escape/']),
    (['escape/none/v0/scalar'], ['--legacy']),
    (['connect/84/unix/disabled/full'], []),
    (['decode-json/ascii-small'], []), (['does-not-exist'], []), ([], []),
])
def test_parallel_cli_rejects_invalid_requests_before_import_or_output(
    runner: Any, tmp_path: Path, case_ids: list[str], extra: list[str],
) -> None:
    output = tmp_path / 'unused'
    command = [sys.executable, runner.__file__, 'parallel', '--threads', '1',
               '--calls', '1', '--cycles', '1', '--output', str(output), *extra]
    for case_id in case_ids:
        command.extend(['--case', case_id])
    result = subprocess.run(command, capture_output=True, text=True, timeout=10)
    assert result.returncode != 0
    assert "invalid choice: 'parallel'" not in result.stderr
    assert 'Traceback' not in result.stderr or 'ValueError' in result.stderr
    assert not output.exists()
    assert 'ModuleNotFoundError' not in result.stderr


def test_free_threaded_entry_rejects_enabled_gil_before_loading_package(
    runner: Any, tmp_path: Path, monkeypatch: pytest.MonkeyPatch,
) -> None:
    def free_threaded_flag(_name: str) -> int:
        return 1

    monkeypatch.setattr(runner.sysconfig, 'get_config_var', free_threaded_flag)
    monkeypatch.setattr(runner.sys, '_is_gil_enabled', lambda: True, raising=False)

    def unexpected_import(package: str | None) -> None:
        raise AssertionError('Must reject the GIL state before native import')

    monkeypatch.setattr(runner, 'load_native', unexpected_import)
    args = Namespace(case=['escape/none/v0/scalar'], prefix=[], legacy=False,
                     reference=None, candidate=None, config=None, threads=1,
                     calls=1, cycles=1, python_work=0, seconds=1, output=str(tmp_path / 'unused'))
    with pytest.raises(RuntimeError, match='GIL disabled before import'):
        runner.parallel(args)
    assert not Path(args.output).exists()


def test_parallel_consumptive_tasks_are_private_and_cleanup_follows_stop(
    runner: Any, monkeypatch: pytest.MonkeyPatch,
) -> None:
    records: list[dict[str, Any]] = []

    def target(case: Any, native: Any, legacy: Any, sessions: dict[str, Any]) -> Any:
        owner = threading.get_ident()
        record: dict[str, Any] = {'owner': owner, 'prepares': [], 'finishes': [], 'calls': 0}
        records.append(record)
        remaining = 0

        class Connection:
            def close(self) -> None:
                assert threading.get_ident() == owner
                record['closed'] = True

        sessions['private'] = Connection()

        def prepare(count: int) -> None:
            nonlocal remaining
            assert threading.get_ident() == owner
            remaining = count
            record['prepares'].append((count, time.perf_counter_ns()))

        def call() -> object:
            nonlocal remaining
            assert threading.get_ident() == owner and remaining > 0
            remaining -= 1
            record['calls'] += 1
            return (42, 'complete')

        def finish() -> None:
            record['finishes'].append(time.perf_counter_ns())

        return runner.Task(call, prepare=prepare, finish=finish)

    monkeypatch.setattr(runner, 'target', target)
    result = runner.parallel_trial(runner.Case('fetch/test', (), 'fetch'), None,
                                   2, 4, 3, 1, time.monotonic() + 5)
    assert result['complete'] is True and result['total_calls'] == 8
    assert len({record['owner'] for record in records}) == 2
    assert len(result['workers']) == 2
    for record in records:
        assert [count for count, _ in record['prepares']] == [1, 1, 4, 1]
        assert record['prepares'][2][1] <= result['wall_start_ns']
        assert record['finishes'][2] >= result['wall_end_ns']
        assert record['calls'] == 7 and record['closed'] is True
    assert result['workers'][0]['output_before'] == result['workers'][1]['output_after']
    assert 'cumulative' in result['rss_scope']


@pytest.mark.parametrize('phase', ['preparation', 'call'])
def test_parallel_worker_failure_aborts_peers_and_closes_sessions(
    runner: Any, monkeypatch: pytest.MonkeyPatch, phase: str,
) -> None:
    closed: list[int] = []
    counter = 0
    lock = threading.Lock()

    def target(case: Any, native: Any, legacy: Any, sessions: dict[str, Any]) -> Any:
        nonlocal counter
        with lock:
            counter += 1
            number = counter

        class Connection:
            def close(self) -> None:
                closed.append(number)

        sessions['private'] = Connection()
        if number == 1 and phase == 'preparation':
            raise ValueError('intentional preparation failure')
        count = 0

        def call() -> object:
            nonlocal count
            count += 1
            if number == 1 and phase == 'call' and count == 3:
                raise ValueError('intentional call failure')
            return 42

        return runner.Task(call)

    monkeypatch.setattr(runner, 'target', target)
    result = runner.parallel_trial(runner.Case('bind/test', (), 'bind'), None,
                                   2, 1, 0, 1, time.monotonic() + 5)
    assert result['complete'] is False
    assert result['alive_workers'] == []
    assert sorted(closed) == [1, 2]
    assert any(f'intentional {phase} failure' in str(error) for error in result['errors'])


def test_parallel_timeout_releases_barriers_and_reports_in_flight_work(
    runner: Any, monkeypatch: pytest.MonkeyPatch,
) -> None:
    release = threading.Event()
    closed = threading.Event()

    def target(case: Any, native: Any, legacy: Any, sessions: dict[str, Any]) -> Any:
        class Connection:
            def close(self) -> None:
                closed.set()

        sessions['private'] = Connection()
        release.wait(timeout=2)
        return runner.Task(lambda: 42)

    monkeypatch.setattr(runner, 'target', target)
    try:
        result = runner.parallel_trial(runner.Case('bind/test', (), 'bind'), None,
                                       1, 1, 0, 1, time.monotonic() + 0.05)
        assert result['complete'] is False and result['alive_workers'] == [0]
        assert 'total_calls' not in result
        assert any(error['type'] == 'TimeoutError' for error in result['errors'])
    finally:
        release.set()
    assert closed.wait(timeout=2)


def test_parallel_binds_each_designated_input_and_keeps_shared_default(
    runner: Any, monkeypatch: pytest.MonkeyPatch,
) -> None:
    roots = [[1, 2], [1, 2]]
    cases = [runner.Case('escape/private', value) for value in roots]
    seen: list[int] = []
    bound: list[int] = []
    closed: list[int] = []

    class Encoded:
        def to_bytes(self) -> bytes:
            return b'(1,2)'

    def target(case: Any, native: Any, legacy: Any, sessions: dict[str, Any]) -> Any:
        seen.append(id(case.value))

        class Connection:
            def close(self) -> None:
                closed.append(id(case.value))

        sessions['owner'] = Connection()
        return runner.Task(Encoded)

    def bind(sql: str, args: tuple[object, ...], **options: object) -> tuple[bytes]:
        bound.append(id(args[0]))
        return (b'\x03SELECT (1,2)',)

    def fingerprint(value: Any) -> dict[str, bytes]:
        return {'value': value.to_bytes()}

    monkeypatch.setattr(runner, 'target', target)
    monkeypatch.setattr(runner, 'fingerprint', fingerprint)
    native = SimpleNamespace(_bind_sql=bind)
    private = runner.parallel_trial(cases[0], native, 2, 4, 0, 1,
                                    time.monotonic() + 5, thread_cases=cases)
    assert private['complete'] and private['total_calls'] == 8
    assert sorted(seen) == sorted(map(id, roots))
    assert sorted(closed) == sorted(map(id, roots))
    assert sorted(bound) == sorted([id(roots[0])] * 2 + [id(roots[1])] * 2)
    assert [worker['input_root_id'] for worker in private['workers']] == list(map(id, roots))
    seen.clear()
    shared = runner.parallel_trial(cases[0], native, 2, 1, 0, 1,
                                   time.monotonic() + 5)
    assert shared['complete'] and seen == [id(roots[0])] * 2


def test_parallel_factory_inputs_are_private_reused_and_checked_for_mutation(
    runner: Any, tmp_path: Path, monkeypatch: pytest.MonkeyPatch,
) -> None:
    generated: list[Any] = []
    used: list[list[int]] = []
    mutate = False

    def factory() -> list[Any]:
        case = runner.Case('bind/private', [1, 2], 'bind')
        generated.append(case)
        return [case]

    def trial(case: Any, native: Any, threads: int, calls: int, python_work: int,
              warmup_calls: int, deadline: float, **options: Any) -> dict[str, Any]:
        assigned = options.get('thread_cases') or [case] * threads
        used.append([id(current.value) for current in assigned])
        if mutate:
            assigned[-1].value.append(3)
        return {'complete': True, 'errors': []}

    module = SimpleNamespace(__file__=str(tmp_path / 'native.so'))

    def native(_package: str | None) -> SimpleNamespace:
        return module

    def identity(_package: str) -> dict[str, str]:
        return {'artifact': module.__file__}

    monkeypatch.setattr(runner, 'cases', factory)
    monkeypatch.setattr(runner, 'load_native', native)
    monkeypatch.setattr(runner, 'package_identity', identity)
    monkeypatch.setattr(runner, 'parallel_trial', trial)
    monkeypatch.setattr(runner, 'gil_state', lambda: {'free_threaded_build': False, 'gil_enabled': True})
    args = Namespace(case=['bind/private'], prefix=[], legacy=False, reference=None,
                     candidate=None, config=None, threads=2, calls=1, cycles=3,
                     python_work=0, seconds=2, warmup_calls=1, package=None,
                     independent_inputs=True, output=str(tmp_path / 'private'))
    runner.parallel(args)
    assert len(generated) == 2 and len(used) == 3
    assert all(ids == used[0] for ids in used) and len(set(used[0])) == 2
    contract = json.loads((Path(args.output) / 'parallel-contract.json').read_text())
    assert contract['input_mode'] == 'independent'
    assert [entry['root_id'] for entry in contract['thread_inputs']] == used[0]
    assert all(entry['case'] == contract['case'] for entry in contract['thread_inputs'])
    args.output = str(tmp_path / 'shared')
    args.independent_inputs = False
    generated.clear()
    used.clear()
    runner.parallel(args)
    assert len(generated) == 1 and all(ids[0] == ids[1] for ids in used)
    args.output = str(tmp_path / 'mutation')
    args.independent_inputs = True
    mutate = True
    with pytest.raises(RuntimeError, match='cycle 0 failed'):
        runner.parallel(args)
    saved = json.loads((Path(args.output) / 'parallel-trials.jsonl').read_text().splitlines()[0])
    assert not saved['complete'] and any('read-only input changed' in error['message'] for error in saved['errors'])


def test_cold_first_observation_precedes_checks_and_default_stays_hot_only(
    runner: Any, monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str],
) -> None:
    events: list[str] = []
    measurements: list[int] = []

    def target(case: Any, native: Any, legacy: Any, sessions: dict[str, Any]) -> Any:
        events.append('create')

        def call() -> int:
            events.append('call')
            return 42

        return runner.Task(call)

    def measure(call: Any, n: int) -> dict[str, int]:
        events.append('measure:' + call.__name__)
        measurements.append(n)
        for _ in range(n):
            call()
        return {'wall_ns': 10, 'cpu_ns': 10}

    def native(_package: str | None) -> SimpleNamespace:
        return SimpleNamespace(__file__=__file__)

    def digest(_path: Path) -> str:
        return 'test-identity'

    def clock() -> dict[str, object]:
        return {}

    monkeypatch.setattr(runner, 'load_native', native)
    monkeypatch.setattr(runner, 'cases', lambda: [runner.Case('bind/cold', [1], 'bind')])
    monkeypatch.setattr(runner, 'digest', digest)
    monkeypatch.setattr(runner, 'clock_calibration', clock)
    monkeypatch.setattr(runner, 'target', target)
    monkeypatch.setattr(runner, 'measure', measure)
    request = {'case': 'bind/cold', 'n': 2, 'role': 'native', 'cold_first': True}
    monkeypatch.setattr(runner.sys, 'stdin', io.StringIO(json.dumps(request) + '\n'))
    runner.worker(None, False, 1)
    result = json.loads(capsys.readouterr().out.splitlines()[-1])
    assert 'error' not in result
    cold = result['cold_first']
    assert measurements == [1, 1, 2]
    assert events[:4] == ['measure:create_task', 'create', 'measure:call', 'call']
    assert events.count('create') == 1 and events.count('call') == 5
    assert cold['imports_included'] is False and cold['input_factory_included'] is False
    assert cold['task_creation']['wall_ns'] == cold['first_full_call']['wall_ns'] == 10
    events.clear()
    measurements.clear()
    request.pop('cold_first')
    monkeypatch.setattr(runner.sys, 'stdin', io.StringIO(json.dumps(request) + '\n'))
    runner.worker(None, False, 1)
    result = json.loads(capsys.readouterr().out.splitlines()[-1])
    assert 'cold_first' not in result and measurements == [2]
    assert events.count('create') == 1 and events.count('call') == 4


def test_cold_first_guards_and_original_creation_error_are_preserved(
    runner: Any, monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str],
) -> None:
    def native(_package: str | None) -> SimpleNamespace:
        return SimpleNamespace(__file__=__file__)

    def digest(_path: Path) -> str:
        return 'test-identity'

    def clock() -> dict[str, object]:
        return {}

    monkeypatch.setattr(runner, 'load_native', native)
    monkeypatch.setattr(runner, 'cases', lambda: [runner.Case('bind/cold', [1], 'bind'),
                                                runner.Case('fetch/cold', [1], 'fetch')])
    monkeypatch.setattr(runner, 'digest', digest)
    monkeypatch.setattr(runner, 'clock_calibration', clock)
    base = {'case': 'bind/cold', 'n': 0, 'role': 'native'}
    cold = {**base, 'cold_first': True}
    creates: list[int] = []
    fail = False
    fail_call = False

    def target(case: Any, native: Any, legacy: Any, sessions: dict[str, Any]) -> Any:
        creates.append(1)
        if fail:
            raise MemoryError('original factory failure')

        def call() -> int:
            if fail_call:
                raise MemoryError('original call failure')
            return 42

        return runner.Task(call)

    monkeypatch.setattr(runner, 'target', target)
    for request in [{**cold, 'role': 'legacy'}, {**cold, 'case': 'fetch/cold'},
                    {**cold, 'prepare': True}, {**cold, 'timing_control': True}]:
        monkeypatch.setattr(runner.sys, 'stdin', io.StringIO(json.dumps(request) + '\n'))
        runner.worker(None, False, 1)
        result = json.loads(capsys.readouterr().out.splitlines()[-1])
        assert result['error'] == 'ValueError' and not creates
    monkeypatch.setattr(runner.sys, 'stdin', io.StringIO(json.dumps(base) + '\n' + json.dumps(cold) + '\n'))
    runner.worker(None, False, 1)
    results = [json.loads(line) for line in capsys.readouterr().out.splitlines()]
    assert results[-1]['error'] == 'ValueError' and len(creates) == 1
    creates.clear()
    fail = True
    monkeypatch.setattr(runner.sys, 'stdin', io.StringIO(json.dumps(cold) + '\n' + json.dumps(cold) + '\n'))
    runner.worker(None, False, 1)
    results = [json.loads(line) for line in capsys.readouterr().out.splitlines()]
    assert results[-2]['error'] == 'MemoryError'
    assert results[-2]['message'] == 'original factory failure'
    assert results[-1]['error'] == 'ValueError' and len(creates) == 1
    creates.clear()
    fail = False
    fail_call = True
    monkeypatch.setattr(runner.sys, 'stdin', io.StringIO(json.dumps(cold) + '\n' + json.dumps(cold) + '\n'))
    runner.worker(None, False, 1)
    results = [json.loads(line) for line in capsys.readouterr().out.splitlines()]
    assert results[-2]['error'] == 'MemoryError' and results[-2]['message'] == 'original call failure'
    assert results[-1]['error'] == 'ValueError' and len(creates) == 1
