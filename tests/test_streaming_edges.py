"""Bounded SS failure/ownership checks on an explicitly selected installed build."""

import pytest

from sqlaero.errors import DataError, InterfaceError, OperationalError, ProgrammingError
from test_public_connection import live, requires_mysql


@requires_mysql
@pytest.mark.parametrize('label', ['84', '97'])
def test_later_err_repeated_nextset_keeps_failure_boundary(label: str) -> None:
    with live(label) as connection:
        cursor = connection.cursor(streaming=True)
        cursor.execute('SELECT 7 AS trusted; SELECT * FROM sqlaero_absent_stream_edges')
        columns = cursor.columns
        assert cursor.fetchall() == ((7,),)
        assert cursor.row_count == cursor.batch_row_count == 1
        with pytest.raises(ProgrammingError) as immediate:
            cursor.nextset()
        original = immediate.value
        assert original.code == 1146 and original.is_server_error is True
        assert cursor.columns is columns
        assert connection.ping() is True
        for _ in range(2):
            with pytest.raises(ProgrammingError) as repeated:
                cursor.nextset()
            assert repeated.value.code == 1146
            assert repeated.value.message == original.message
        cursor.close()
        assert cursor.closed and connection.ping() is True


@requires_mysql
@pytest.mark.parametrize('label', ['84', '97'])
def test_complete_buffered_batch_prefix_stays_local_after_stream_connection_close(label: str) -> None:
    connection = live(label)
    cursor = connection.cursor(streaming=True)
    cursor.executemany('SELECT %s AS value', [(10,), (20,), (30,)])
    first_columns = cursor.columns
    assert cursor.row_count == 1
    connection.close()
    assert cursor.fetchall() == ((10,),)
    assert cursor.columns is first_columns
    assert cursor.nextset() is True
    assert cursor.fetchall() == ((20,),)
    with pytest.raises(InterfaceError):
        cursor.nextset()
    cursor.close()


@requires_mysql
@pytest.mark.parametrize('label', ['84', '97'])
def test_single_message_exceeding_result_capacity_retires_unfinished_stream(label: str) -> None:
    connection = live(label, result_memory_limit=32 * 1024)
    cursor = connection.cursor(streaming=True)
    cursor.execute("SELECT REPEAT('x', 131072) AS payload")
    columns = cursor.columns
    with pytest.raises(OperationalError) as immediate:
        cursor.fetchone()
    assert immediate.value.is_server_error is False
    assert connection.closed
    assert cursor.row_count is None and cursor.batch_row_count is None
    assert cursor.columns is columns
    for _ in range(2):
        with pytest.raises(OperationalError) as repeated:
            cursor.fetchone()
        assert repeated.value.code == immediate.value.code
    cursor.close()
    assert cursor.closed
    connection.close()


@requires_mysql
@pytest.mark.parametrize('label', ['84', '97'])
def test_rejected_closed_nextset_keeps_complete_current_remaining_rows(label: str) -> None:
    connection = live(label)
    cursor = connection.cursor(streaming=True)
    cursor.executemany('SELECT %s AS value UNION ALL SELECT %s+1', [(10, 10), (20, 20)])
    columns = cursor.columns
    assert cursor.fetchone() == (10,)
    connection.close()
    with pytest.raises(InterfaceError):
        cursor.nextset()
    assert cursor.columns is columns and cursor.row_count == 2
    assert cursor.fetchall() == ((11,),)
    with pytest.raises(InterfaceError):
        cursor.nextset()
    cursor.close()


@requires_mysql
@pytest.mark.parametrize('label', ['84', '97'])
@pytest.mark.parametrize('complete', [False, True])
def test_closed_decode_cache_requires_the_entire_set_to_be_complete(label: str, complete: bool) -> None:
    connection = live(label, sql_mode='')
    cursor = connection.cursor(streaming=True)
    cursor.execute("SELECT CAST('0000-00-00' AS DATE) AS value UNION ALL SELECT CAST('2024-01-01' AS DATE)")
    columns = cursor.columns
    operation = cursor.fetchall if complete else cursor.fetchone
    with pytest.raises(DataError):
        operation()
    assert cursor.row_count == (2 if complete else None)
    connection.close()
    for _ in range(2):
        with pytest.raises(DataError if complete else InterfaceError):
            operation()
    assert cursor.columns is columns
    cursor.close()


@requires_mysql
@pytest.mark.parametrize('label', ['84', '97'])
def test_failed_later_binding_keeps_trusted_ss_prefix_local_after_close(label: str) -> None:
    connection = live(label)
    cursor = connection.cursor(streaming=True)
    with pytest.raises(Exception) as immediate:
        cursor.executemany('SELECT %s AS value', [(7,), (object(),)])
    original = immediate.value
    assert cursor.row_count == 1 and cursor.batch_row_count is None
    assert not connection.closed
    connection.close()
    assert cursor.fetchall() == ((7,),)
    for _ in range(2):
        with pytest.raises(type(original)) as boundary:
            cursor.nextset()
        assert str(boundary.value) == str(original)
    cursor.close()
