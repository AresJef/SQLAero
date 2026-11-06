"""Auxiliary SQL projection is independent of command and result lifetimes."""

from typing import Any, cast

import pytest
from sqlaero import Connection

from test_public_connection import connection as connection, live, requires_mysql


@requires_mysql
def test_public_projection_preserves_roles_and_result_position(connection: Connection) -> None:
    with connection.cursor() as cur:
        cur.execute("SELECT 10 AS a UNION ALL SELECT 11; SELECT 12 AS b")
        columns = cur.columns
        assert connection.escape([1, None, "x"]) == ("1", "NULL", "'x'")
        assert connection.escape([(1,), (2,)], many=True) == [("1",), ("2",)]
        assert not hasattr(cur, "escape")
        assert cur.mogrify("SELECT %s AS v", None) == "SELECT NULL AS v"
        assert cur.mogrify("SELECT '%s', '%%'") == "SELECT '%s', '%%'"
        assert cur.mogrify("SELECT %s AS v", [(1,), (2,)], many=True) == [
            "SELECT 1 AS v", "SELECT 2 AS v"
        ]
        assert cur.mogrify("INSERT INTO t(v) VALUES (%s)", [(1,), (2,)], many=True) == (
            "INSERT INTO t(v) VALUES (1),(2)"
        )
        assert cur.columns is columns
        assert cur.fetchone() == (10,)
        assert cur.nextset() is True
        assert cur.fetchone() == (12,)


@requires_mysql
def test_auxiliary_functions_remain_local_after_close(connection: Connection) -> None:
    cur = connection.cursor()
    cur.execute("SELECT 7")
    cur.close()
    connection.close()
    assert connection.escape("value") == "'value'"
    assert connection.escape(42) == "42"
    assert cur.mogrify("SELECT %s", 3) == "SELECT 3"


@requires_mysql
@pytest.mark.parametrize("value", [None, 0, 1, "true", []])
def test_projection_switches_are_exact_bool(connection: Connection, value: object) -> None:
    with connection.cursor() as cur:
        for name in ("many", "itemize"):
            # Invalid switches intentionally reach the runtime exact-bool guard.
            with pytest.raises(TypeError, match=name):
                connection.escape(1, **{name: cast(Any, value)})
            with pytest.raises(TypeError, match=name):
                cur.mogrify("SELECT %s", 1, **{name: cast(Any, value)})


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
def test_sjis_projection_refuses_only_nonlossless_output(label: str) -> None:
    with live(label, charset="sjis") as conn:
        with conn.cursor() as cur:
            assert conn.escape("表") == "'表'"
            with pytest.raises(ValueError, match="sjis.*losslessly"):
                conn.escape("'")
            with pytest.raises(ValueError, match="sjis.*losslessly"):
                cur.mogrify("SELECT %s", "'")
            # The approved limitation does not change the real execution path.
            assert cur.execute("SELECT HEX(%s)", "'") == 1
            assert cur.fetchone() == ("27",)


@requires_mysql
def test_projection_does_not_use_operation_timeout_or_result_quota(connection: Connection) -> None:
    previous = connection.operation_timeout
    try:
        connection.operation_timeout = 1e-9
        value = "x" * 300_000
        assert connection.escape(value) == "'" + value + "'"
        with connection.cursor() as cur:
            assert cur.mogrify("SELECT %s", value) == "SELECT '" + value + "'"
    finally:
        connection.operation_timeout = previous
