"""Check the fixed-work parallel entry without collecting performance evidence."""
from __future__ import annotations

import importlib.util
from argparse import Namespace
from pathlib import Path
import subprocess
import sys
import threading
import time
from typing import Any

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
