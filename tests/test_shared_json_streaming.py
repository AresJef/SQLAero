"""Native JSON fragments preserve one SQL value, control and actual facts."""

# pyright: reportPrivateUsage=false
from datetime import datetime, timedelta, tzinfo
import json
from typing import NoReturn

import pytest

from sqlaero import JSON, Cursor, DictCursor, SSCursor, SSDictCursor, _sqlaero, json_dumps
from sqlaero.errors import OperationalError, SQLAeroErrorCode
from test_public_connection import live, requires_mysql


def test_utf8_packet_capacity_stops_before_a_later_json_source_protocol() -> None:
    calls: list[str] = []
    original = RuntimeError("later source must remain unread after request capacity fails")
    class Zone(tzinfo):
        def utcoffset(self, value: datetime | None) -> NoReturn:
            calls.append("offset")
            raise original
    payload = ["x" * (1024 * 1024), datetime(2026, 1, 1, tzinfo=Zone())]
    with pytest.raises(OperationalError) as caught:
        _sqlaero._bind_sql("SELECT %s", (JSON(payload),), limit=4096)
    assert caught.value.code == SQLAeroErrorCode.RESOURCE_LIMIT
    assert caught.value.is_server_error is False
    assert calls == []
    # The independent public document has no SQL packet quota. This proves
    # the later protocol is reachable, with its original exception intact.
    with pytest.raises(RuntimeError) as independent:
        json_dumps(payload)
    assert independent.value is original and calls == ["offset"]


def test_non_utf8_keeps_the_complete_document_content_error() -> None:
    document = '{"first":"é","last":"🙂"}'
    with pytest.raises(UnicodeEncodeError) as caught:
        _sqlaero._bind_sql("SELECT %s", (JSON({"first":"é","last":"🙂"}),), charset="latin1")
    assert caught.value.object == document
    index = document.index("🙂")
    assert (caught.value.start, caught.value.end) == (index, index + 1)


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("kind", [Cursor, DictCursor, SSCursor, SSDictCursor])
@pytest.mark.parametrize("prepared", [False, True])
@pytest.mark.parametrize("no_backslash", [False, True])
def test_large_fragmented_json_has_the_same_real_database_value(
    label: str, kind: type[Cursor] | type[DictCursor] | type[SSCursor] | type[SSDictCursor],
    prepared: bool, no_backslash: bool,
) -> None:
    text = "é中🙂'\\\"\0\n" * 20000
    payload = {"long":text, "rows":[7, None, {"tail":"end"}]}
    with live(label) as connection:
        with connection.cursor(kind, prepared=prepared) as current:
            if no_backslash:
                current.execute("SET SESSION sql_mode='NO_BACKSLASH_ESCAPES'")
            current.decode_json = True
            current.execute("SELECT CAST(%s AS JSON) AS document", (JSON(payload),))
            expected = {"document":payload} if kind in (DictCursor, SSDictCursor) else (payload,)
            assert current.fetchone() == expected
            assert current.fetchone() is None
            assert json.loads(json_dumps(payload)) == payload


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("kind", [Cursor, DictCursor, SSCursor, SSDictCursor])
def test_late_fragment_capacity_failure_preserves_the_real_confirmed_prefix(
    label: str, kind: type[Cursor] | type[DictCursor] | type[SSCursor] | type[SSDictCursor],
) -> None:
    calls: list[str] = []
    class Zone(tzinfo):
        def utcoffset(self, value: datetime | None) -> timedelta:
            calls.append("unexpected offset")
            return timedelta(0)
    values = [JSON({"id":7}), JSON(["x" * (1024 * 1024), datetime(2026, 1, 1, tzinfo=Zone())])]
    with live(label, max_allowed_packet="4KB") as connection:
        with connection.cursor() as setup:
            setup.execute("CREATE TEMPORARY TABLE json_fragment_prefix (document LONGTEXT)")
        with connection.cursor(kind) as current:
            with pytest.raises(OperationalError) as caught:
                current.execute("INSERT INTO json_fragment_prefix SELECT %s", values, many=True)
            assert caught.value.code == SQLAeroErrorCode.RESOURCE_LIMIT
            assert caught.value.is_server_error is False
            assert calls == []
            assert current.row_count == 1 and current.batch_row_count is None
            with pytest.raises(OperationalError):
                current.nextset()
        with connection.cursor() as check:
            check.execute("SELECT document FROM json_fragment_prefix")
            assert check.fetchall() == (("{\"id\":7}",),)
            check.execute("SELECT 42")
            assert check.fetchone() == (42,)
