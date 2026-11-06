"""Public streaming owners, current counts and atomic output on real MySQL."""

import gc
import time
from typing import TypeAlias

import pytest
import sqlaero
from sqlaero.errors import DataError, InterfaceBusyError, InterfaceError, NotSupportedError
from test_public_connection import live, requires_mysql


CursorFactory: TypeAlias = (
    None | type[tuple[object, ...]] | type[dict[str, object]]
    | type[sqlaero.Cursor] | type[sqlaero.DictCursor]
    | type[sqlaero.SSCursor] | type[sqlaero.SSDictCursor]
)


def row(value: int, dictionary: bool) -> tuple[int] | dict[str, int]:
    return {"v": value} if dictionary else (value,)


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("dictionary", [False, True])
def test_streaming_header_unknown_count_and_busy_until_actual_completion(label: str, dictionary: bool) -> None:
    with live(label) as connection:
        with connection.cursor(dict if dictionary else tuple, streaming=True) as cursor:
            assert type(cursor) is (sqlaero.SSDictCursor if dictionary else sqlaero.SSCursor)
            assert cursor.execute("SELECT 1 AS v UNION ALL SELECT 2 UNION ALL SELECT 3") is None
            assert cursor.rowcount == -1 and cursor.row_count is None
            assert cursor.batch_row_count is None
            columns = cursor.columns
            assert columns.names == ("v",)
            with connection.cursor() as other:
                with pytest.raises(InterfaceBusyError):
                    other.execute("SELECT 9")
            with pytest.raises(InterfaceBusyError):
                connection.set_multi_statements(False)
            assert cursor.fetchone() == row(1, dictionary)
            assert cursor.row_count is None
            assert cursor.fetchmany(0) == ()
            assert cursor.fetchall() == (row(2, dictionary), row(3, dictionary))
            assert cursor.row_count == cursor.rowcount == cursor.batch_row_count == 3
            assert cursor.fetchone() is None
            assert cursor.columns is columns
            assert connection.ping() is True
            assert cursor.nextset() is None


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("dictionary", [False, True])
def test_nextset_discards_only_current_response_and_exposes_command_states(label: str, dictionary: bool) -> None:
    with live(label) as connection, connection.cursor(dict if dictionary else tuple, streaming=True) as cursor:
        assert cursor.execute("SELECT 1 AS v UNION ALL SELECT 2; SET @ss_marker=41; SELECT @ss_marker+1 AS v") is None
        assert cursor.fetchone() == row(1, dictionary)
        assert cursor.nextset() is True
        assert cursor.description is None and cursor.row_count == 0
        assert cursor.batch_row_count == 2
        assert cursor.nextset() is True
        assert cursor.row_count is None
        assert cursor.fetchall() == (row(42, dictionary),)
        assert cursor.row_count == 1 and cursor.batch_row_count == 2
        assert cursor.nextset() is None
        assert cursor.fetchone() is None
        assert connection.ping() is True


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
def test_explicit_cursor_close_drains_and_keeps_connection_but_connection_close_does_not(label: str) -> None:
    with live(label) as connection:
        cursor = connection.cursor(streaming=True)
        cursor.execute("SELECT 1 AS v UNION ALL SELECT 2; SELECT 3")
        metadata = cursor.columns
        assert cursor.fetchone() == (1,)
        cursor.close()
        assert cursor.closed and cursor.row_count == 2 and cursor.columns is metadata
        assert connection.closed is False and connection.ping() is True
        current = connection.cursor(streaming=True)
        current.execute("SELECT 4 AS v UNION ALL SELECT 5")
        connection.close()
        assert connection.closed
        with pytest.raises(InterfaceError, match="unfinished.*closed"):
            current.fetchone()
        current.close()


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
def test_dropped_unread_cursor_retires_connection_without_cleanup_io(label: str) -> None:
    connection = live(label)
    cursor = connection.cursor(streaming=True)
    cursor.execute("SELECT 1 UNION ALL SELECT 2")
    columns = cursor.columns
    del cursor
    gc.collect()
    assert connection.closed is True
    assert columns.names == ("1",)
    connection.close()


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("dictionary", [False, True])
def test_all_batch_requests_execute_before_return_and_final_request_stays_streamed(label: str, dictionary: bool) -> None:
    with live(label) as connection, connection.cursor(dict if dictionary else tuple, streaming=True) as cursor:
        assert cursor.executemany("SELECT %s AS v", [(10,), (20,), (30,)]) is None
        assert cursor.row_count == 1
        assert cursor.batch_row_count is None
        for index, value in enumerate((10, 20, 30)):
            assert cursor.fetchall() == (row(value, dictionary),)
            assert cursor.nextset() is (True if index < 2 else None)
        assert cursor.batch_row_count == 3
        assert connection.ping() is True


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
def test_forward_scroll_never_decodes_skipped_rows_and_keeps_actual_out_of_range_position(label: str) -> None:
    with live(label, sql_mode="") as connection, connection.cursor(streaming=True) as cursor:
        cursor.execute("SELECT CAST('0000-00-00' AS DATE) AS v UNION ALL SELECT CAST('2024-01-01' AS DATE)")
        assert cursor.scroll(1) is None
        from datetime import date
        assert cursor.fetchone() == (date(2024, 1, 1),)
        assert cursor.scroll(0) is None
        with pytest.raises(IndexError, match="actually skipped to position 2"):
            cursor.scroll(100)
        assert cursor.fetchone() is None
        with pytest.raises(NotSupportedError):
            cursor.scroll(0, mode="absolute")


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
def test_decode_failure_keeps_native_row_and_does_not_replay_sql(label: str) -> None:
    with live(label, sql_mode="") as connection, connection.cursor(streaming=True) as cursor:
        cursor.execute("SELECT CAST('0000-00-00' AS DATE) AS v")
        columns = cursor.columns
        for _ in range(2):
            with pytest.raises(DataError) as caught:
                cursor.fetchone()
            assert any("row 0" in note and "position remains 0" in note for note in caught.value.__notes__)
            assert cursor.columns is columns
        cursor.close()
        assert connection.ping() is True


@requires_mysql
def test_user_idle_and_later_timeout_assignment_do_not_reset_existing_stream_policy() -> None:
    with live("84", operation_timeout=0.5) as connection, connection.cursor(streaming=True) as cursor:
        cursor.execute("SELECT 1 AS v UNION ALL SELECT 2")
        time.sleep(0.6)
        connection.operation_timeout = 1e-9
        connection.read_timeout = 1e-9
        try:
            assert cursor.fetchall() == ((1,), (2,))
        finally:
            connection.operation_timeout = 0.5
            connection.read_timeout = None


@requires_mysql
@pytest.mark.parametrize("selector", [None, tuple, dict, sqlaero.Cursor, sqlaero.DictCursor, sqlaero.SSCursor, sqlaero.SSDictCursor])
def test_streaming_selector_override_preserves_format(selector: CursorFactory) -> None:
    with live("84") as connection:
        dictionary = selector in (dict, sqlaero.DictCursor, sqlaero.SSDictCursor)
        for streaming in (True, False):
            with connection.cursor(selector, streaming=streaming) as cursor:
                expected = (sqlaero.SSDictCursor if dictionary else sqlaero.SSCursor) if streaming else (sqlaero.DictCursor if dictionary else sqlaero.Cursor)
                assert type(cursor) is expected
                cursor.execute("SELECT 7 AS v")
                assert cursor.fetchone() == row(7, dictionary)
