"""Feature recipes reuse the frozen sampler and retain their real call work."""
from __future__ import annotations

import base64
from collections.abc import Callable
from dataclasses import dataclass, field
import hashlib
import importlib.util
import inspect
import io
import json
import os
from pathlib import Path
import sys
from types import SimpleNamespace
from typing import Any
import zlib

import pytest


@pytest.fixture
def runner() -> Any:
    path = Path(__file__).resolve().parents[1] / "tools/benchmark.py"
    spec = importlib.util.spec_from_file_location("_feature_boundary_benchmark", path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def test_existing_case_inputs_and_ten_core_functions_are_unchanged(runner: Any) -> None:
    frozen = {
        "cases": "6a296a8093a8576895fe722cc6ec3b558060da2f822c36539ee7f3b21cd93663",
        "typed_cases": "3d294e7de8028743ad1d0e0f1faead4eef4dad2707732ea6ae4569a6783eeaeb",
        "network_target": "b67d30c957b8dc3bfd6ae3f9235a17cbfa8ddaf605bc0784e5bbb2898afe6198",
        "clock_calibration": "4b5380cf4fe535e03e7df62ce0de893baf8615b7ad5e37265be588e5ce922565",
        "measure": "b877a7159d2fcd2f4dd25c397bf610e3500d53798447e6b5439b35ccba1fe025",
        "calibrated_sample": "b2a40cdae4d3e2498bbad1ca527ce41087db3513f7733ccaeb42fa9ffdfa6eea",
        "control": "e3c3a0b9b4d8874fb2f777ab71dd2ddb0dbdec1e59599ccce165a60d70a82abe",
        "quality": "44f169315fe7faddd02ad59166720c24aeff46ba99fac3a06675908bb4630c61",
        "summarize": "f495c442b2a3ed2500a9822d51ab6c86fa92e20c26fec28e36b7d835c79594ef",
        "smoke": "11aa1d8ca0c7424af1c854d52439cf3949b1c5cd3b8a8cedc9ee2891f8ff5fc9",
    }
    for name, expected in frozen.items():
        assert hashlib.sha256(inspect.getsource(getattr(runner, name)).encode()).hexdigest() == expected
    original = [case.identity() for case in runner.cases(typed=False)]
    assert len(original) == 490
    assert hashlib.sha256(json.dumps(original, sort_keys=True).encode()).hexdigest() == (
        "01bcb23851de5ba7576e079e2df04b486a27ff0f7fe01a514ee0132c169c7ebe")


def test_feature_inputs_have_fifteen_network_recipes_per_server(runner: Any) -> None:
    items = runner.feature_cases()
    assert len(items) == len({case.id for case in items}) == 30
    assert {case.family for case in items} == {"feature"}
    assert "feature" not in runner.CPU_FAMILIES
    assert len([case for case in items if case.id.startswith("feature/84/")]) == 15
    assert len([case for case in items if case.id.startswith("feature/97/")]) == 15
    payloads: dict[str, bytes] = {}
    for case in items:
        assert case.id.startswith("feature/")
        if case.value["recipe"] == "compression":
            name = case.id.split("/")[-2]
            value = case.value["payload"]
            assert type(value) is bytes and len(value) == 262144
            assert payloads.setdefault(name, value) == value
    assert len(zlib.compress(payloads["repeated"])) < 262144 // 100
    assert len(zlib.compress(payloads["incompressible"])) > 262144 * 0.99
    assert len([case for case in items if case.value.get("local_infile")]) == 2


def test_tls_inputs_are_six_cpu_recipes_without_changing_network_inputs(runner: Any) -> None:
    items = runner.tls_cases()
    assert len(items) == len({case.id for case in items}) == 6
    assert all(case.id.startswith("feature/tls/") and case.family == "tls" for case in items)
    assert "tls" in runner.CPU_FAMILIES and "feature" not in runner.CPU_FAMILIES
    assert len(runner.feature_cases()) == 30
    for case in items:
        with pytest.raises(ValueError, match="never a legacy no-op"):
            runner.target(case, None, object(), {})


@pytest.mark.skipif('SQLAERO_TEST_MYSQL_ROOT' not in os.environ,
                    reason='requires explicitly selected owned MySQL fixtures')
def test_tls_six_recipes_use_installed_native_and_owned_mysql_oracles(
    runner: Any, record_property: Callable[[str, object], None],
) -> None:
    """Exercise real public TLS recipes untimed, separately from the doubles."""
    from importlib.machinery import EXTENSION_SUFFIXES
    from importlib.metadata import distribution

    import sqlaero

    package = Path(sqlaero.__file__).resolve().parent
    source = Path(__file__).resolve().parents[1]
    assert not package.is_relative_to(source / 'python'), 'select an installed package, not the development extension'
    installed = Path(str(distribution('sqlaero').locate_file('sqlaero'))).resolve()
    assert package == installed, 'the imported package differs from the installed distribution'
    spec = importlib.util.find_spec('sqlaero._sqlaero')
    assert spec is not None and spec.origin is not None
    artifact = Path(spec.origin).resolve()
    assert artifact.parent == installed
    assert artifact.name.endswith(tuple(EXTENSION_SUFFIXES)), 'the real native extension is required'
    record_property('tls_recipe_native', str(artifact))

    # mysql_fixture.py creates this one leaf with DNS:localhost. Derive the
    # expected bytes/fingerprint from its public PEM, independently of TLSInfo.
    fixture = Path(os.environ['SQLAERO_TEST_MYSQL_ROOT'])
    pem = (fixture / 'tls/server.pem').read_text(encoding='ascii').splitlines()
    der = base64.b64decode(''.join(pem[1:-1]), validate=True)
    expected: dict[str, object] = {
        'ssl/ca-memory': ('TLSv1.3', 'TLSv1.3'),
        'ssl/ca-file': ('TLSv1.3', 'TLSv1.3'),
        'warm/version': 'TLSv1.3',
        'warm/certificate-dns': ('localhost',),
        'warm/certificate-der': der,
        'warm/certificate-sha256': hashlib.sha256(der).hexdigest(),
    }
    items = runner.tls_cases()
    assert [case.value['recipe'] for case in items] == list(expected)
    sessions: dict[str, Any] = {}
    completed: list[str] = []
    try:
        for case in items:
            task = runner.target(case, sqlaero, None, sessions)
            assert task.prepare is None and task.finish is None
            for _ in range(2):
                assert task.call() == expected[case.value['recipe']], case.id
            completed.append(case.id)
        assert len(completed) == 6
    finally:
        for connection in sessions.values():
            connection.close()
        record_property('tls_recipe_completed', completed)


@pytest.fixture
def tls_boundary_native(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch,
) -> tuple[SimpleNamespace, list[str], bytes]:
    """A public-interface double checks recipe boundaries, not TLS correctness."""
    (tmp_path / "84").mkdir()
    (tmp_path / "84/state.json").write_text(json.dumps({"test_password": "fixture-only", "port": 15384}))
    (tmp_path / "tls").mkdir()
    (tmp_path / "tls/ca.pem").write_bytes(b"fixture CA input")
    der = b"independent fixture leaf DER"
    (tmp_path / "tls/server.pem").write_text(
        "-----BEGIN CERTIFICATE-----\n" + base64.b64encode(der).decode("ascii")
        + "\n-----END CERTIFICATE-----\n", encoding="ascii")
    monkeypatch.setenv("SQLAERO_TEST_MYSQL_ROOT", str(tmp_path))
    events: list[str] = []
    certificate = SimpleNamespace(dns_names=("localhost",), to_der=lambda: der,
                                  fingerprint_sha256=hashlib.sha256(der).hexdigest())
    info = SimpleNamespace(mode="VERIFY_IDENTITY", version="TLSv1.3", server_name="localhost",
                           peer_certificates=(certificate,))

    def ssl(**settings: Any) -> SimpleNamespace:
        events.append("construct")
        assert settings["min_tls_version"] == settings["max_tls_version"] == "TLSv1.3"
        return SimpleNamespace(min_tls_version=settings["min_tls_version"],
                               max_tls_version=settings["max_tls_version"])

    def connect(**settings: Any) -> SimpleNamespace:
        events.append("connect")
        assert settings["host"] == "127.0.0.1" and settings["ssl"] is not None
        return SimpleNamespace(tls_info=info, close=lambda: events.append("close"))

    return SimpleNamespace(SSL=ssl, connect=connect), events, der


@pytest.mark.parametrize("recipe", ["ssl/ca-memory", "ssl/ca-file"])
def test_tls_construction_remains_in_every_call_with_connection_setup_outside(
    runner: Any, tls_boundary_native: tuple[SimpleNamespace, list[str], bytes], recipe: str,
) -> None:
    native, events, _der = tls_boundary_native
    case = next(case for case in runner.tls_cases() if case.value["recipe"] == recipe)
    task = runner.target(case, native, None, {})
    assert events.count("connect") == 1
    before = events.count("construct")
    for _ in range(4):
        assert task.call() == ("TLSv1.3", "TLSv1.3")
    assert events.count("construct") == before + 4
    assert events.count("connect") == 1
    assert task.prepare is None and task.finish is None


@pytest.mark.parametrize("recipe", ["warm/version", "warm/certificate-dns",
                                    "warm/certificate-der", "warm/certificate-sha256"])
def test_tls_snapshot_calls_use_warmed_public_values_without_network_work(
    runner: Any, tls_boundary_native: tuple[SimpleNamespace, list[str], bytes], recipe: str,
) -> None:
    native, events, der = tls_boundary_native
    expected: dict[str, object] = {"warm/version": "TLSv1.3",
                                   "warm/certificate-dns": ("localhost",),
                                   "warm/certificate-der": der,
                                   "warm/certificate-sha256": hashlib.sha256(der).hexdigest()}
    case = next(case for case in runner.tls_cases() if case.value["recipe"] == recipe)
    task = runner.target(case, native, None, {})
    before = events[:]
    for _ in range(4):
        assert task.call() == expected[recipe]
    assert events == before, "TLS preparation or connection work entered the getter call"
    assert task.prepare is None and task.finish is None


def test_physical_feature_sessions_include_all_negotiated_context(
    runner: Any, tmp_path: Path, monkeypatch: pytest.MonkeyPatch,
) -> None:
    (tmp_path / "84").mkdir()
    (tmp_path / "84/state.json").write_text(json.dumps({"test_password": "owned-test-only", "port": 15384}))
    monkeypatch.setenv("SQLAERO_TEST_MYSQL_ROOT", str(tmp_path))
    calls: list[dict[str, Any]] = []

    def connect(**settings: Any) -> Any:
        calls.append(settings)
        algorithm = settings["compression"]
        actual = (None if algorithm is None
                  else SimpleNamespace(algorithms=(algorithm,), zstd_compression_level=3))
        tls = settings.get("ssl")
        return SimpleNamespace(compression=actual, tls_info=None if tls is None else tls, close=lambda: None)

    def ssl(**settings: Any) -> SimpleNamespace:
        return SimpleNamespace(**settings)

    native = SimpleNamespace(connect=connect, SSL=ssl)
    common = {"server": "84", "transport": "unix", "tls": "disabled",
              "compression": None, "local_infile": False}
    sessions: dict[str, Any] = {}
    first = runner.feature_connection(common, native, sessions)
    assert runner.feature_connection(common, native, sessions) is first
    for change in ({"transport": "tcp"}, {"tls": "verified", "transport": "tcp"},
                   {"compression": "zlib"}, {"compression": "zstd"}, {"local_infile": True}):
        assert runner.feature_connection({**common, **change}, native, sessions) is not first
    assert len(calls) == len(sessions) == 6
    assert calls[-1]["local_infile"] is True
    assert all("owned-test-only" not in key for key in sessions)
    case = runner.feature_cases()[0]
    with pytest.raises(ValueError, match="never a legacy no-op"):
        runner.target(case, native, object(), {})


@dataclass
class _PreparedConnection:
    handles: list[_PreparedCursor] = field(default_factory=lambda: list[_PreparedCursor]())
    events: list[tuple[str, int, int]] = field(default_factory=lambda: list[tuple[str, int, int]]())

    def cursor(self, *, prepared: bool) -> _PreparedCursor:
        assert prepared is True
        value = _PreparedCursor(self, len(self.handles))
        self.handles.append(value)
        return value

    def close(self) -> None:
        assert all(value.closed for value in self.handles)


@dataclass
class _PreparedCursor:
    connection: _PreparedConnection
    identifier: int
    executions: int = 0
    closed: bool = False

    def execute(self, sql: str, args: object) -> int:
        assert not self.closed and sql == "SELECT %s AS v" and args == (7,)
        self.connection.events.append(("execute", self.identifier, self.executions))
        self.executions += 1
        return 1

    def fetchall(self) -> tuple[tuple[int]]:
        return ((7,),)

    def nextset(self) -> None:
        return None

    def close(self) -> None:
        assert not self.closed
        self.connection.events.append(("close", self.identifier, self.executions))
        self.closed = True


@pytest.mark.parametrize("mode", ["first", "reuse"])
def test_prepared_measure_batches_keep_first_prepare_and_close_on_the_correct_side(
    runner: Any, monkeypatch: pytest.MonkeyPatch, mode: str,
) -> None:
    connection = _PreparedConnection()

    def physical(*_args: object) -> _PreparedConnection:
        return connection

    monkeypatch.setattr(runner, "feature_connection", physical)
    case = next(case for case in runner.feature_cases() if case.id == f"feature/84/prepared/{mode}/R1")
    task = runner.target(case, None, None, {})
    assert all(value.closed for value in connection.handles)
    assert task.prepare_each_role is False
    for n in (3, 2):
        before = len(connection.handles)
        task.prepare(n)
        batch = connection.handles[before:]
        assert len(batch) == (n if mode == "first" else 1)
        assert [value.executions for value in batch] == ([0] * n if mode == "first" else [1])
        for _ in range(n):
            assert task.call() == (1, ((7,),), None)
        assert [value.executions for value in batch] == ([1] * n if mode == "first" else [n + 1])
        assert not any(value.closed for value in batch), "CLOSE entered the measured call"
        task.finish()
        assert all(value.closed for value in batch)


def test_worker_checks_and_warmup_cannot_turn_first_prepare_into_reuse(
    runner: Any, monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str],
) -> None:
    connection = _PreparedConnection()
    case = next(case for case in runner.feature_cases() if case.id == "feature/84/prepared/first/R1")

    def physical(_spec: Any, _native: Any, sessions: dict[str, Any]) -> _PreparedConnection:
        sessions["owned"] = connection
        return connection

    measured: list[list[tuple[str, int, int]]] = []

    def measure(call: Callable[[], object], n: int) -> dict[str, int]:
        start = len(connection.events)
        for _ in range(n):
            call()
        measured.append(connection.events[start:])
        return {"n": n, "wall_ns": 1, "cpu_ns": 1, "snapshot_ns": 0}

    monkeypatch.setattr(runner, "feature_connection", physical)
    def load(_package: str | None) -> SimpleNamespace:
        return SimpleNamespace(__file__=__file__)

    def usage(_scope: int) -> SimpleNamespace:
        return SimpleNamespace(ru_maxrss=0)

    monkeypatch.setattr(runner, "cases", lambda: list[Any]())
    monkeypatch.setattr(runner, "feature_cases", lambda: [case])
    monkeypatch.setattr(runner, "load_native", load)
    monkeypatch.setattr(runner, "clock_calibration", lambda: {"unit-test": True})
    monkeypatch.setattr(runner, "measure", measure)
    monkeypatch.setattr(runner, "resource", SimpleNamespace(RUSAGE_SELF=0,
                        getrusage=usage))
    requests = [{"case": case.id, "prepare": 3, "n": 3, "each_role": False},
                {"case": case.id, "n": 3}]
    monkeypatch.setattr(runner.sys, "stdin", io.StringIO("\n".join(json.dumps(row) for row in requests)))
    runner.worker(None, False, warmup_calls=2)
    rows = [json.loads(line) for line in capsys.readouterr().out.splitlines()]
    assert not any("error" in row for row in rows)
    assert len(measured) == 1 and len(measured[0]) == 3
    assert [event[2] for event in measured[0]] == [0, 0, 0]
    assert len({event[1] for event in measured[0]}) == 3
    assert all(value.closed for value in connection.handles)


@dataclass
class _LocalConnection:
    rows: list[tuple[int, str]] = field(default_factory=lambda: list[tuple[int, str]]())
    handles: list[_LocalCursor] = field(default_factory=lambda: list[_LocalCursor]())
    uploads: list[Path] = field(default_factory=lambda: list[Path]())

    def cursor(self) -> _LocalCursor:
        value = _LocalCursor(self)
        self.handles.append(value)
        return value

    def close(self) -> None:
        assert all(value.closed for value in self.handles)


@dataclass
class _LocalCursor:
    connection: _LocalConnection
    closed: bool = False

    def __enter__(self) -> _LocalCursor:
        return self

    def __exit__(self, *_args: object) -> None:
        self.close()

    def execute(self, sql: str, *, local_file: Path | None = None) -> int:
        assert not self.closed
        if sql.startswith("CREATE TEMPORARY TABLE"):
            assert "PRIMARY KEY" not in sql and "UNIQUE" not in sql
            return 0
        if sql.startswith("TRUNCATE"):
            self.connection.rows.clear()
            return 0
        if sql.startswith("LOAD DATA LOCAL INFILE"):
            assert local_file is not None and local_file.is_file()
            self.connection.uploads.append(local_file)
            # Only this unit's fake public API reads Python bytes. The real
            # Task passes the existing Path to Rust execute(local_file=...).
            values = [line.split("\t") for line in local_file.read_text().splitlines()]
            self.connection.rows.extend((int(number), value) for number, value in values)
            return len(values)
        assert sql == "SELECT n,s FROM sqlaero_feature_local ORDER BY n"
        return len(self.connection.rows)

    def fetchall(self) -> tuple[tuple[int, str], ...]:
        return tuple(sorted(self.connection.rows))

    def close(self) -> None:
        assert not self.closed
        self.closed = True


def test_local_batches_reopen_the_same_file_and_support_full_n_and_two_n_work(
    runner: Any, monkeypatch: pytest.MonkeyPatch,
) -> None:
    connection = _LocalConnection()

    def physical(*_args: object) -> _LocalConnection:
        return connection

    monkeypatch.setattr(runner, "feature_connection", physical)
    case = next(case for case in runner.feature_cases() if case.id == "feature/84/local/N10000")
    task = runner.target(case, None, None, {})
    assert not connection.rows and all(value.closed for value in connection.handles)
    for n in (2, 4):
        task.prepare(n)
        for _ in range(n):
            assert task.call() == 10000
        paths = connection.uploads[-n:]
        assert len(set(paths)) == 1 and paths[0].is_file()
        assert len(connection.rows) == n * 10000, "an upload was omitted or the table cleared inside call"
        assert paths[0].read_bytes() == case.value["payload"]
        task.finish()
        assert not paths[0].exists() and not connection.rows
    connection.close()
