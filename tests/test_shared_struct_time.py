"""The two targets consume only the struct_time facts they actually require."""

from datetime import datetime
import json
from time import struct_time

import pytest

from sqlaero import Cursor, DictCursor, SSCursor, SSDictCursor, _sqlaero, json_dumps
from sqlaero.errors import InterfaceError, SQLAeroErrorCode
from test_public_connection import live, requires_mysql


FIELDS = ("tm_year", "tm_mon", "tm_mday", "tm_hour", "tm_min", "tm_sec",
          "tm_wday", "tm_yday", "tm_isdst", "tm_zone", "tm_gmtoff")
BASE = (2024, 1, 2, 3, 4, 5, 1, 2, -1)


@pytest.mark.parametrize("offset", [None, 0, 28800, -18000, 1.5, 86400])
def test_struct_time_json_preserves_all_eleven_original_fields(offset: object) -> None:
    value = struct_time(BASE, {"tm_zone": "source zone", "tm_gmtoff": offset})
    expected = dict(zip(FIELDS, (*BASE, "source zone", offset)))
    assert json.loads(json_dumps(value)) == expected
    assert json.loads(json_dumps([value])) == [expected]


@pytest.mark.parametrize(("parts", "offset", "expected"), [
    (BASE, None, b"'2024-01-02 03:04:05'"),
    (BASE, 0, b"'2024-01-02 03:04:05'"),
    (BASE, 28800, b"'2024-01-01 19:04:05'"),
    (BASE, -18000, b"'2024-01-02 08:04:05'"),
    ((2024, 3, 1, 0, 0, 0, 0, 0, 0), 1, b"'2024-02-29 23:59:59'"),
    ((2000, 2, 29, 23, 59, 59, 0, 0, 0), -1, b"'2000-03-01 00:00:00'"),
    ((10000, 1, 1, 0, 0, 0, 0, 0, 0), 1, b"'9999-12-31 23:59:59'"),
    ((0, 1, 1, 0, 0, 0, 0, 0, 0), 1, b"'-0001-12-31 23:59:59'"),
    ((2024, 13, 31, 25, 59, 61, 999, 999, 999), None, b"'2024-13-31 25:59:61'"),
])
def test_struct_time_mysql_uses_the_offset_only_when_present(parts: tuple[int, ...], offset: object, expected: bytes) -> None:
    value = struct_time(parts, {"tm_zone": object(), "tm_gmtoff": offset})
    assert _sqlaero._encode_value(value).to_bytes() == expected


@pytest.mark.parametrize(("parts", "offset", "error"), [
    ((2024, 13, 2, 3, 4, 5, 0, 0, 0), 0, ValueError),
    ((1900, 2, 29, 3, 4, 5, 0, 0, 0), 0, ValueError),
    ((2024, 1, 2, 24, 0, 0, 0, 0, 0), 0, ValueError),
    ((2024, 1, 2, 0, 0, 60, 0, 0, 0), 1, ValueError),
    (BASE, 86400, ValueError),
    (BASE, -86400, ValueError),
    (BASE, 1.5, TypeError),
    (BASE, "0", TypeError),
])
def test_struct_time_aware_calendar_and_offset_are_not_silently_repaired(parts: tuple[int, ...], offset: object, error: type[Exception]) -> None:
    with pytest.raises(error):
        _sqlaero._encode_value(struct_time(parts, {"tm_gmtoff": offset}))


def test_struct_time_uses_base_int_values_and_preserves_index_protocol_errors() -> None:
    class Year(int):
        def __index__(self) -> int:
            raise AssertionError("must use actual int storage")
        def __int__(self) -> int:
            raise AssertionError("must use actual int storage")
    value = struct_time((Year(2024), 1, 2, 3, 4, 5, 1, 2, -1))
    assert _sqlaero._encode_value(value).to_bytes() == b"'2024-01-02 03:04:05'"
    assert json.loads(json_dumps(value))["tm_year"] == 2024
    original = MemoryError("offset protocol original")
    class Offset:
        def __index__(self) -> int:
            raise original
    with pytest.raises(MemoryError) as caught:
        _sqlaero._encode_value(struct_time(BASE, {"tm_gmtoff": Offset()}))
    assert caught.value is original
    assert "tm_gmtoff" in "\n".join(caught.value.__notes__)


def test_struct_time_json_errors_point_to_the_actual_unused_sql_field() -> None:
    value = struct_time(BASE, {"tm_zone": object()})
    assert _sqlaero._encode_value(value).to_bytes() == b"'2024-01-02 03:04:05'"
    with pytest.raises(TypeError) as caught:
        json_dumps({"clock": value})
    assert "data.clock.tm_zone" in "\n".join(caught.value.__notes__)


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("kind", [Cursor, DictCursor, SSCursor, SSDictCursor])
@pytest.mark.parametrize("prepared", [False, True])
def test_struct_time_utc_database_result_and_session_guard(label: str, kind, prepared: bool) -> None:
    value = struct_time(BASE, {"tm_gmtoff": 28800})
    with live(label) as connection, connection.cursor(kind, prepared=prepared) as current:
        current.execute("SELECT CAST(%s AS DATETIME) AS stamp", (value,))
        expected = datetime(2024, 1, 1, 19, 4, 5)
        assert current.fetchone() == ({"stamp": expected} if kind in (DictCursor, SSDictCursor) else (expected,))
        assert current.fetchone() is None
        current.execute("SET SESSION time_zone = '+02:00'")
        with pytest.raises(InterfaceError) as caught:
            current.execute("SELECT %s", (value,))
        assert caught.value.code == SQLAeroErrorCode.SESSION_STATE
        current.execute("SELECT 17 AS value")
        assert current.fetchone() == ({"value": 17} if kind in (DictCursor, SSDictCursor) else (17,))
