"""Host/container administration shares one private fixture client contract."""

import json
from collections.abc import Iterator
from contextlib import contextmanager
from pathlib import Path
from types import SimpleNamespace

import pytest

from tools import mysql_fixture


def test_administration_uses_the_client_registered_with_the_instance(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(mysql_fixture, 'ROOT', tmp_path)
    directory = tmp_path / '84'
    directory.mkdir()
    client = tmp_path / 'mysql-client-84'
    (directory / 'state.json').write_text(json.dumps({'mysql': str(client)}))
    # No installed host server binary is needed when an owned container bridge
    # is the registered client. Both Rust and Python consume this same path.
    def unexpected_program(label: str, name: str) -> Path:
        pytest.fail(f'must not resolve a different native client: {label}/{name}')

    monkeypatch.setattr(mysql_fixture, 'program', unexpected_program)
    assert mysql_fixture.mysql_command('84', 'test') == [
        str(client), f'--defaults-file={directory / "test.cnf"}',
        '--no-login-paths', '--batch', '--raw', '--skip-column-names',
    ]


def test_unix_readiness_waits_for_its_socket_even_when_tcp_is_open(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch,
) -> None:
    unix_ready = False
    tcp_calls: list[tuple[str, int]] = []
    unix_calls: list[str] = []
    timeouts: list[float] = []
    closed: list[bool] = []

    class UnixProbe:
        def settimeout(self, timeout: float) -> None:
            timeouts.append(timeout)

        def connect(self, endpoint: str) -> None:
            unix_calls.append(endpoint)
            if not unix_ready:
                raise FileNotFoundError(endpoint)

    @contextmanager
    def tcp(endpoint: tuple[str, int], *, timeout: float) -> Iterator[None]:
        assert timeout == 0.2
        tcp_calls.append(endpoint)
        yield None

    @contextmanager
    def unix(family: int, kind: int) -> Iterator[UnixProbe]:
        assert (family, kind) == (1, 2)
        try:
            yield UnixProbe()
        finally:
            closed.append(True)

    fake = SimpleNamespace(create_connection=tcp, socket=unix, AF_UNIX=1, SOCK_STREAM=2)
    monkeypatch.setattr(mysql_fixture, 'network', fake)
    endpoint = tmp_path / 'mysql.sock'
    assert mysql_fixture.ready_endpoint(13384, None) is True
    assert mysql_fixture.ready_endpoint(13384, endpoint) is False
    unix_ready = True
    assert mysql_fixture.ready_endpoint(13384, endpoint) is True
    assert tcp_calls == [('127.0.0.1', 13384)]
    assert unix_calls == [str(endpoint), str(endpoint)]
    assert timeouts == [0.2, 0.2] and closed == [True, True]


def test_windows_selected_none_uses_only_tcp(monkeypatch: pytest.MonkeyPatch) -> None:
    calls: list[tuple[tuple[str, int], float]] = []

    @contextmanager
    def tcp(endpoint: tuple[str, int], *, timeout: float) -> Iterator[None]:
        calls.append((endpoint, timeout))
        yield None

    # This module-level replacement deliberately has no Unix socket API.
    monkeypatch.setattr(mysql_fixture, 'network', SimpleNamespace(create_connection=tcp))
    assert mysql_fixture.ready_endpoint(13397, None) is True
    assert calls == [(('127.0.0.1', 13397), 0.2)]
