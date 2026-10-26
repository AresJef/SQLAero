"""Public maintained IntEnums, native exception identity, and Unicode messages."""

from enum import IntEnum
import json
from pathlib import Path
from typing import Any

from sqlaero.errors import MySQLErrorCode, SQLAeroErrorCode, SQLAeroError, IntegrityError


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
