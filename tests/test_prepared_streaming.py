"""Prepared rows and statement release share the real streaming ownership path."""

import gc

import pytest
import sqlaero
from sqlaero.errors import InterfaceBusyError, InterfaceError
from test_public_connection import live, requires_mysql


@requires_mysql
@pytest.mark.parametrize('label', ['84', '97'])
@pytest.mark.parametrize('dictionary', [False, True])
@pytest.mark.parametrize('compression', [None, 'zlib', 'zstd'])
def test_prepared_stream_rows_busy_reuse_and_compressed_close(
    label: str, dictionary: bool, compression: str | None,
) -> None:
    with live(label, compression=compression) as connection:
        cursor = connection.cursor(dict if dictionary else tuple, streaming=True, prepared=True)
        assert type(cursor) is (sqlaero.SSDictCursor if dictionary else sqlaero.SSCursor)
        assert cursor.prepared is True
        assert cursor.execute('SELECT %s AS v UNION ALL SELECT %s', (1, 2)) is None
        columns = cursor.columns
        assert cursor.row_count is None and cursor.batch_row_count is None
        assert cursor.fetchone() == ({'v': 1} if dictionary else (1,))
        with connection.cursor() as other:
            with pytest.raises(InterfaceBusyError):
                other.execute('SELECT 99')
        assert cursor.fetchall() == (({'v': 2} if dictionary else (2,)),)
        assert cursor.row_count == cursor.batch_row_count == 2
        assert cursor.nextset() is None
        assert cursor.execute('SELECT %s AS v UNION ALL SELECT %s', (3, 4)) is None
        assert cursor.fetchall() == tuple({'v': v} if dictionary else (v,) for v in (3, 4))
        assert columns.names == ('v',)
        # A different source replaces the one handle and must send CLOSE even
        # when framing is compressed, without a fabricated acknowledgement.
        assert cursor.execute('SELECT %s AS changed', (5,)) is None
        assert cursor.fetchall() == (({'changed': 5} if dictionary else (5,)),)
        cursor.close()
        assert cursor.closed and cursor.prepared and connection.ping()


@requires_mysql
@pytest.mark.parametrize('label', ['84', '97'])
@pytest.mark.parametrize('compression', [None, 'zlib', 'zstd'])
@pytest.mark.parametrize('garbage_collect', [False, True])
def test_completed_prepared_handle_release_does_not_disturb_another_unread_stream(
    label: str, compression: str | None, garbage_collect: bool,
) -> None:
    with live(label, compression=compression) as connection:
        prepared = connection.cursor(prepared=True)
        prepared.execute('SELECT %s AS saved', (7,))
        assert prepared.fetchall() == ((7,),)
        streaming = connection.cursor(streaming=True)
        streaming.execute('SELECT 8 AS active UNION ALL SELECT 9')
        if garbage_collect:
            del prepared
            gc.collect()
        else:
            prepared.close()
            assert prepared.closed
        assert connection.closed is False
        assert streaming.fetchall() == ((8,), (9,))
        streaming.close()
        # The next legal command flushes native IDs under its own time budget.
        assert connection.ping() is True
        with connection.cursor() as observer:
            observer.execute("SHOW SESSION STATUS LIKE 'Com_stmt_close'")
            result = observer.fetchone()
            assert result is not None and int(result[1]) == 1


@requires_mysql
@pytest.mark.parametrize('label', ['84', '97'])
@pytest.mark.parametrize('dictionary', [False, True])
def test_prepared_stream_batch_prefix_and_transaction_drain(label: str, dictionary: bool) -> None:
    with live(label) as connection:
        with connection.transaction(dict if dictionary else tuple, prepared=True, streaming=True) as cursor:
            assert cursor.executemany('SELECT %s AS v', [(1,), (2,), (3,)]) is None
            assert cursor.fetchall() == (({'v': 1} if dictionary else (1,)),)
            assert cursor.nextset() is True
            assert cursor.fetchall() == (({'v': 2} if dictionary else (2,)),)
            assert cursor.nextset() is True
            assert cursor.row_count is None
            # Transaction exit drains the remaining binary response before
            # control, then releases this Cursor's handle under the same cap.
        assert cursor.closed and cursor.batch_row_count == 3
        assert connection.ping() is True


@requires_mysql
@pytest.mark.parametrize('label', ['84', '97'])
def test_prepared_unfinished_owner_gc_retires_without_flushing_a_close(label: str) -> None:
    connection = live(label)
    cursor = connection.cursor(prepared=True, streaming=True)
    cursor.execute('SELECT %s UNION ALL SELECT %s', (1, 2))
    columns = cursor.columns
    del cursor
    gc.collect()
    assert connection.closed is True and columns.names == ('?',)
    connection.close()


@requires_mysql
@pytest.mark.parametrize('label', ['84', '97'])
def test_prepared_mogrify_is_readable_and_independent_of_pending_network_response(label: str) -> None:
    with live(label) as connection:
        streaming = connection.cursor(streaming=True)
        streaming.execute('SELECT 1 UNION ALL SELECT 2')
        preview = connection.cursor(prepared=True)
        assert preview.mogrify('SELECT %s AS v', [(3,), (4,)], many=True) == [
            'SELECT 3 AS v', 'SELECT 4 AS v',
        ]
        connection.close()
        assert preview.mogrify('SELECT %s', (5,)) == 'SELECT 5'
        with pytest.raises(InterfaceError):
            streaming.fetchone()
        streaming.close()
        preview.close()
