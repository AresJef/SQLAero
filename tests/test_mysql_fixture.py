"""Host/container administration shares one private fixture client contract."""

import json
from collections.abc import Generator
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
    def tcp(endpoint: tuple[str, int], *, timeout: float) -> Generator[None, None, None]:
        assert timeout == 0.2
        tcp_calls.append(endpoint)
        yield None

    @contextmanager
    def unix(family: int, kind: int) -> Generator[UnixProbe, None, None]:
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
    def tcp(endpoint: tuple[str, int], *, timeout: float) -> Generator[None, None, None]:
        calls.append((endpoint, timeout))
        yield None

    # This module-level replacement deliberately has no Unix socket API.
    monkeypatch.setattr(mysql_fixture, 'network', SimpleNamespace(create_connection=tcp))
    assert mysql_fixture.ready_endpoint(13397, None) is True
    assert calls == [(('127.0.0.1', 13397), 0.2)]


@pytest.mark.parametrize('partial_revokes', [False, True])
def test_callproc_grants_are_exact_schema_and_two_table_selects(partial_revokes: bool) -> None:
    sql = mysql_fixture.feature_grants('sqlaero_ci_call', partial_revokes)
    expected = 'sqlaero_ci_call``database' if partial_revokes else r'sqlaero\_ci\_call``database'
    assert sql.count(f'ON `{expected}`.*') == 2
    assert sql.count('ON `performance_schema`.`user_variables_by_thread`') == 2
    assert sql.count('ON `performance_schema`.`threads`') == 2
    assert "TO 'sqlaero'@'localhost'" in sql and "TO 'sqlaero'@'127.0.0.1'" in sql
    assert 'ON *.*' not in sql and 'GRANT ALL' not in sql and 'GRANT OPTION' not in sql
    assert 'CREATE ROUTINE, ALTER ROUTINE, EXECUTE' in sql
    assert mysql_fixture.feature_grants('sqlaero_ci_call', partial_revokes) == sql


@pytest.mark.parametrize('prefix', ['', 'x%wild', 'x`inject', 'x\\wild', 'a'*33])
def test_callproc_grant_namespace_rejects_untrusted_patterns(prefix: str) -> None:
    with pytest.raises(ValueError):
        mysql_fixture.feature_grants(prefix, False)


def test_feature_environment_uses_only_provisioned_openssl_and_the_exact_owned_root(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch,
) -> None:
    owned = tmp_path / 'owned'
    owned.mkdir()
    executable = tmp_path / 'existing-openssl.exe'
    executable.write_text('fixture executable, never run')
    executable.chmod(0o700)
    monkeypatch.setattr(mysql_fixture, 'ROOT', owned)
    monkeypatch.setenv('SQLAERO_OPENSSL', str(executable))
    output = tmp_path / 'github-env'
    output.write_text('UNCHANGED=1\n')
    mysql_fixture.feature_environment(output)
    assert output.read_text() == (f'UNCHANGED=1\nSQLAERO_OPENSSL={executable.resolve()}\n'
                                  f'SQLAERO_TEST_MUTABLE_TLS_ROOT={owned}\n')


def test_missing_openssl_is_a_preparation_failure_without_download(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    def missing(_selected: str) -> None:
        return None

    monkeypatch.delenv('SQLAERO_OPENSSL', raising=False)
    monkeypatch.setattr(mysql_fixture.shutil, 'which', missing)
    with pytest.raises(RuntimeError, match='SQLAERO_OPENSSL'):
        mysql_fixture.openssl_program()
