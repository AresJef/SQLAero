"""Host/container administration shares one private fixture client contract."""

import json
from pathlib import Path

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
