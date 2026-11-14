"""Actual UUID/text and JSON writes through both protocols and all Cursors."""

from uuid import UUID, uuid4

import pytest

from sqlaero import JSON, Cursor, DictCursor, SSCursor, SSDictCursor
from test_public_connection import live, requires_mysql


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("kind", [Cursor, DictCursor, SSCursor, SSDictCursor])
@pytest.mark.parametrize("prepared", [False, True])
@pytest.mark.parametrize(("number", "expected"), [
    (0, "00000000-0000-0000-0000-000000000000"),
    (0x12345678123456781234567812345678, "12345678-1234-5678-1234-567812345678"),
    ((1 << 128) - 1, "ffffffff-ffff-ffff-ffff-ffffffffffff"),
], ids=["zero", "sample", "maximum"])
def test_actual_uuid_and_json_column_values(label: str, kind: type[Cursor] | type[DictCursor] | type[SSCursor] | type[SSDictCursor], prepared: bool, number: int, expected: str) -> None:
    table = "uuid_write_" + uuid4().hex[:12]
    with live(label) as connection:
        with connection.cursor() as setup:
            setup.execute(f"CREATE TEMPORARY TABLE `{table}` (u CHAR(36), doc JSON)")
        with connection.cursor(kind, prepared=prepared) as current:
            current.decode_json = True
            value = UUID(int=number)
            assert current.execute(f"INSERT INTO `{table}` VALUES (%s,%s)", (value, JSON({"u": value}))) == 1
            current.execute(f"SELECT u AS value,doc FROM `{table}`")
            expected_row = ({"value": expected, "doc": {"u": expected}}
                            if kind in (DictCursor, SSDictCursor) else (expected, {"u": expected}))
            assert current.fetchone() == expected_row
            assert current.fetchone() is None
