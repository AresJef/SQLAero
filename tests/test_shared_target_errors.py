"""Target resource failures keep the same typed client error through codecs."""

# pyright: reportPrivateUsage=false
from typing import Any


import pytest

from sqlaero import Cursor, DictCursor, SSCursor, SSDictCursor, _sqlaero
from sqlaero.errors import OperationalError, SQLAeroErrorCode
from test_public_connection import live, requires_mysql


@pytest.mark.parametrize("source", ["oversized", "中文", type("ActualText", (str,), {})("oversized")])
def test_scalar_text_capacity_preserves_the_resource_error_class_and_code(source: Any) -> None:
    with pytest.raises(OperationalError) as caught:
        _sqlaero._encode_scalar(source, limit=1)
    assert caught.value.code == SQLAeroErrorCode.RESOURCE_LIMIT
    assert caught.value.is_server_error is False
    assert _sqlaero._encode_scalar("ok") == b"'ok'"


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("kind", [Cursor, DictCursor, SSCursor, SSDictCursor])
def test_public_text_expansion_limit_is_a_resource_error_before_any_request(label: str, kind: type[Cursor] | type[DictCursor] | type[SSCursor] | type[SSDictCursor]) -> None:
    # The lower bound fits 2KiB. Actual SQL quoting expands it beyond 2KiB.
    source = "'" * 1500
    with live(label, max_allowed_packet="2KB") as connection:
        with connection.cursor() as setup:
            setup.execute("SET SESSION sql_mode=''")
            setup.execute("CREATE TEMPORARY TABLE resource_boundary (value TEXT)")
        with connection.cursor(kind) as current:
            current.execute("SELECT 41 AS value UNION ALL SELECT 42")
            columns = current.columns
            literal = "'" + "\\'" * 1500 + "'"
            # Escape is a fragment; mogrify plans actual requests and therefore
            # retains the request capacity bound without executing those SQLs.
            assert connection.escape(source) == literal
            with pytest.raises(OperationalError) as preview_error:
                current.mogrify("SELECT %s", (source,))
            assert preview_error.value.code == SQLAeroErrorCode.RESOURCE_LIMIT
            assert preview_error.value.is_server_error is False
            assert current.columns is columns
            expected = ({"value": 41}, {"value": 42}) if kind in (DictCursor, SSDictCursor) else ((41,), (42,))
            assert current.fetchall() == expected
            with pytest.raises(OperationalError) as caught:
                current.execute("INSERT INTO resource_boundary VALUES (%s)", (source,))
            assert caught.value.code == SQLAeroErrorCode.RESOURCE_LIMIT
            assert caught.value.is_server_error is False
        with connection.cursor() as observer:
            observer.execute("SELECT COUNT(*) FROM resource_boundary")
            assert observer.fetchone() == (0,)
            observer.execute("SELECT 42")
            assert observer.fetchone() == (42,)
