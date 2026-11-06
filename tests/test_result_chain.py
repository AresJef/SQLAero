"""Final result-set, retained-prefix and navigation contracts on installed wheels."""

from typing import SupportsIndex, cast

import pytest

from sqlaero import Connection
from sqlaero.errors import ProgrammingError
from test_public_connection import connection as connection, requires_mysql


@requires_mysql
def test_current_set_counts_and_all_results_are_independent(connection: Connection) -> None:
    with connection.cursor() as cur:
        assert hasattr(cur, "nextset")
        assert cur.execute("SELECT 10 AS first UNION ALL SELECT 11; SELECT 20 AS second") == 2
        assert cur.row_count == cur.rowcount == cur.affected_rows == 2
        assert cur.batch_row_count == 2
        first_columns = cur.columns
        assert first_columns.names == ("first",)
        assert cur.fetchone() == (10,)
        assert cur.nextset() is True
        assert cur.row_count == 1
        assert cur.columns.names == ("second",)
        assert first_columns.names == ("first",)
        assert cur.fetchall() == ((20,),)
        assert cur.nextset() is None
        assert cur.row_count == 1
        assert cur.columns.names == ("second",)
        assert cur.fetchone() is None
        assert cur.fetchall() == ()


@requires_mysql
def test_all_batch_requests_are_retained_and_first_counts_are_summed(connection: Connection) -> None:
    with connection.cursor() as cur:
        assert cur.executemany("SELECT %s AS value", [(10,), (20,), (30,)]) == 3
        assert cur.batch_row_count == 3
        for index, value in enumerate((10, 20, 30)):
            assert cur.row_count == cur.rowcount == 1
            assert cur.fetchall() == ((value,),)
            assert cur.nextset() is (True if index < 2 else None)


@requires_mysql
def test_later_server_error_keeps_trusted_prefix_and_failure_boundary(connection: Connection) -> None:
    with connection.cursor() as cur:
        with pytest.raises(ProgrammingError) as caught:
            cur.execute("SELECT 7 AS trusted; SELECT * FROM sqlaero_absent_result_chain_table")
        assert caught.value.code == 1146
        assert cur.row_count == 1
        assert cur.fetchall() == ((7,),)
        with pytest.raises(type(caught.value)) as boundary:
            cur.nextset()
        assert boundary.value.code == caught.value.code
        assert boundary.value.is_server_error is True
        assert "sqlaero_absent_result_chain_table" in str(boundary.value)
        with connection.cursor() as other:
            assert other.execute("SELECT 9") == 1
            assert other.fetchone() == (9,)


@requires_mysql
def test_local_preflight_failure_does_not_replace_old_chain(connection: Connection) -> None:
    with connection.cursor() as cur:
        cur.execute("SELECT 1 AS first; SELECT 2 AS second")
        old_columns = cur.columns
        with pytest.raises(Exception):
            cur.execute("SELECT %s", [])
        assert cur.columns is old_columns
        assert cur.fetchone() == (1,)
        assert cur.nextset() is True
        assert cur.fetchone() == (2,)


@requires_mysql
def test_buffered_navigation_keeps_native_rows_at_eof(connection: Connection) -> None:
    class Index:
        def __index__(self) -> int:
            return 0

    with connection.cursor() as cur:
        cur.execute("SELECT 1 AS value UNION ALL SELECT 2")
        columns = cur.columns
        assert cur.fetchall() == ((1,), (2,))
        assert cur.scroll(Index(), mode="absolute") is None
        assert cur.fetchone() == (1,)
        assert cur.scroll(1) is None
        assert cur.fetchone() is None
        with pytest.raises(IndexError):
            cur.scroll(1)
        assert cur.fetchone() is None
        for invalid in (True, 1.5, "1", None):
            with pytest.raises(TypeError):
                # Deliberate runtime rejection cases remain unchanged objects.
                cur.scroll(cast(SupportsIndex, invalid))
        connection.close()
        cur.scroll(0, mode="absolute")
        assert cur.fetchall() == ((1,), (2,))
        assert cur.columns is columns
