"""Public maintained IntEnums, native exception identity, and Unicode messages."""

from enum import IntEnum
from copy import deepcopy
import json
from pathlib import Path
from typing import Any

import pytest
from sqlaero.errors import MySQLErrorCode, SQLAeroErrorCode, SQLAeroError, IntegrityError


def reviewed_expectations() -> dict[str, dict[int, str]]:
    """Load a fixed audit oracle which the product generator cannot rewrite."""
    path = Path(__file__).parent / 'data/error-classification-audit-20261026.tsv'
    result: dict[str, dict[int, str]] = {'client': {}, 'server': {}}
    for line in path.read_text().splitlines():
        if line.startswith('#'):
            continue
        origin, code, category, evidence, rationale = line.split('\t')
        assert evidence and rationale and int(code) not in result[origin]
        result[origin][int(code)] = category
    assert len(result['server']) == 2046 and len(result['client']) == 19
    return result


def test_complete_registry_matches_independent_reviewed_semantics() -> None:
    source: dict[str, Any] = json.loads((Path(__file__).resolve().parents[1] / 'data/error-catalog.json').read_text())
    expected = reviewed_expectations()
    for origin in ('client', 'server'):
        actual = {row['code']: row['class'] for row in source[origin]}
        assert actual == expected[origin]


@pytest.mark.parametrize(('number', 'explanation'), [
    (2, 'A file read failed; the server message includes the file and operating-system cause'),
    (1308, 'LEAVE or ITERATE refers to a label with no matching target'),
    (1320, 'A stored function contains no RETURN statement'),
    (1321, 'A stored function reached its end without executing RETURN'),
    (1337, 'Local variables and conditions must be declared before cursors and handlers'),
    (1338, 'Cursors must be declared before handlers in a stored program'),
    (1886, 'The table lacks an index required by the requested operation'),
    (3600, 'Window processing requires the TempTable internal temporary-memory storage engine'),
    (3838, 'The plugin must not be loaded through the early-plugin interface'),
    (6421, 'Lakehouse incremental loading cannot be combined with validation mode'),
    (6446, 'A JSON duality view definition has invalid, duplicate or conflicting annotations'),
])
def test_enum_documents_condition_not_abbreviated_identifier(number: int, explanation: str) -> None:
    """Keep independently reviewed meanings, including misleading legacy names."""
    member = MySQLErrorCode(number)
    assert member.__doc__ is not None and explanation in member.__doc__


def test_condition_fingerprint_detects_english_continuations_without_copying_templates() -> None:
    from tools import error_catalog

    source = 'ER_EXAMPLE\n eng "first"\n  "second"\n fre "autre"\n'
    original = error_catalog.message_fingerprints(source)
    assert original == error_catalog.message_fingerprints(source.replace('autre', 'différent'))
    assert original != error_catalog.message_fingerprints(source.replace('second', 'changed'))
    assert len(original['ER_EXAMPLE']) == 64


@pytest.mark.parametrize('change', ['none', 'added', 'removed', 'message', 'state'])
def test_fact_refresh_preserves_reviewed_semantics_or_refuses_atomic_update(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, change: str,
) -> None:
    from tools import error_catalog

    maintained: dict[str, Any] = json.loads((Path(__file__).resolve().parents[1] / 'data/error-catalog.json').read_text())
    row = next(entry for entry in maintained['server'] if entry['code'] == 1052)
    row['class'] = 'ProgrammingError'
    row['basis'] = 'Reviewed column ambiguity, not a relationship constraint failure.'
    row['meaning'] = 'reviewed public meaning retained on fact refresh'
    for entry in maintained['server']:
        for facts in entry['versions'].values():
            facts['message_sha256'] = 'a' * 64
    registry = tmp_path / 'error-catalog.json'
    registry.write_text(json.dumps(maintained, ensure_ascii=False))
    before = registry.read_bytes()
    versions = {version: {entry['code']: deepcopy(entry['versions'][version])
                          for entry in maintained['server'] if version in entry['versions']}
                for version in ('8.4', '9.7')}
    if change == 'added':
        versions['9.7'][65535] = {'name': 'ER_UNREVIEWED', 'sqlstate': '23000', 'message_sha256': 'b' * 64}
    elif change == 'removed':
        versions['9.7'].pop(1052)
    elif change == 'message':
        versions['9.7'][1052]['message_sha256'] = 'b' * 64
    elif change == 'state':
        versions['9.7'][1052]['sqlstate'] = 'HY000'

    def installed(version: str, label: str) -> tuple[dict[int, dict[str, str]], list[dict[str, str]], list[dict[str, Any]]]:
        return versions[version], [], []

    def globals_for(label: str) -> tuple[dict[int, dict[str, str]], dict[str, str]]:
        return {}, {}

    monkeypatch.setattr(error_catalog, 'REGISTRY', registry)
    monkeypatch.setattr(error_catalog, 'installed', installed)
    monkeypatch.setattr(error_catalog, 'globals_for', globals_for)
    if change != 'none':
        with pytest.raises(ValueError, match='review required'):
            error_catalog.snapshot()
        assert registry.read_bytes() == before
    else:
        error_catalog.snapshot()
        refreshed = json.loads(registry.read_text())
        assert refreshed['server'] == maintained['server']
        assert refreshed['client'] == maintained['client']


def test_public_enums_match_all_frozen_values_aliases_and_individual_docs() -> None:
    source: dict[str, Any] = json.loads((Path(__file__).resolve().parents[1] / 'data/error-catalog.json').read_text())
    assert issubclass(MySQLErrorCode, IntEnum) and issubclass(SQLAeroErrorCode, IntEnum)
    for row in source['client']:
        member = SQLAeroErrorCode.__members__[row['name']]
        assert int(member) == row['code']
        assert member.__doc__ and row['class'] in member.__doc__ and str(row['code']) in member.__doc__
    for row in source['server']:
        aliases = {entry['name'] for entry in row['versions'].values()}
        for name in aliases:
            member = MySQLErrorCode.__members__[name]
            assert int(member) == row['code']
            assert member.__doc__ and row['class'] in member.__doc__ and name in member.__doc__
    assert MySQLErrorCode.ER_DUP_ENTRY == 1062
    assert SQLAeroErrorCode.NUMERIC_RANGE > 65535
    assert MySQLErrorCode.ER_LH_INFER_SCHEMA_MISMATCH is MySQLErrorCode.ER_LH_INFER_NUM_COL_MISMATCH


def test_enum_constructor_codes_stay_plain_int_and_preserve_manual_independence() -> None:
    error = IntegrityError('duplicate', code=MySQLErrorCode.ER_DUP_ENTRY, is_server_error=True)
    assert type(error.code) is int and error.code == 1062
    assert str(error) == 'Server Error [1062]: duplicate'
    manual = SQLAeroError('manual', code=MySQLErrorCode.ER_DUP_ENTRY, is_server_error=False)
    assert manual.code == 1062 and manual.is_server_error is False


def test_exception_message_preserves_python_unicode_without_utf8_roundtrip() -> None:
    message = 'raw\udcff\x00é中'
    value = SQLAeroError(message)
    assert value.message == message and value.args == (message,) and str(value) == message
    assert value.message is value.args[0]
    server = SQLAeroError(message, code=1062, is_server_error=True)
    assert str(server) == 'Server Error [1062]: ' + message
    assert server.message == message
