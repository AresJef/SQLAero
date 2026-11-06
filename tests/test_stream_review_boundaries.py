"""Public stream deadlines and synchronized publication failure boundaries."""

from collections.abc import Iterator
import time
from uuid import uuid4

import pytest
from sqlaero import Connection, SSCursor, SSDictCursor
from sqlaero.errors import (
    OperationalError, OperationalTimeoutError, SQLAeroError, SQLAeroErrorCode,
)
from test_public_connection import live, requires_mysql


StreamingCursor = SSCursor | SSDictCursor
_OPERATION_SECONDS = 0.5
_INDEX_SECONDS = 0.8
_READER_LIMIT = 52 * 1024


def _cursor(connection: Connection, dictionary: bool) -> StreamingCursor:
    return (connection.cursor(dict, streaming=True) if dictionary
            else connection.cursor(streaming=True))


class _SlowIndex:
    def __init__(self, value: int) -> None:
        self.value = value
        self.calls = 0
        self.elapsed = 0.0

    def __index__(self) -> int:
        self.calls += 1
        start = time.monotonic()
        time.sleep(_INDEX_SECONDS)
        self.elapsed += time.monotonic() - start
        return self.value


@requires_mysql
@pytest.mark.parametrize('label', ['84', '97'])
@pytest.mark.parametrize('dictionary', [False, True])
@pytest.mark.parametrize('target', [0, 1])
def test_incomplete_scroll_counts_index_protocol_time_inside_the_library_call(
    label: str, dictionary: bool, target: int,
) -> None:
    with live(label, operation_timeout=_OPERATION_SECONDS) as connection:
        current = _cursor(connection, dictionary)
        try:
            assert current.execute('SELECT 7 AS value UNION ALL SELECT 8') is None
            assert current.row_count is None
            columns = current.columns
            index = _SlowIndex(target)
            with pytest.raises(OperationalTimeoutError) as caught:
                current.scroll(index)
            assert index.calls == 1 and index.elapsed >= _INDEX_SECONDS
            assert caught.value.code == SQLAeroErrorCode.OPERATION_DEADLINE
            assert caught.value.is_server_error is False
            assert current.columns is columns
            assert current.row_count is current.batch_row_count is None
            assert connection.closed is True
        finally:
            current.close()


@requires_mysql
@pytest.mark.parametrize('label', ['84', '97'])
@pytest.mark.parametrize('dictionary', [False, True])
def test_complete_local_stream_scroll_does_not_inherit_the_old_sql_budget(
    label: str, dictionary: bool,
) -> None:
    with live(label, operation_timeout=_OPERATION_SECONDS) as connection:
        current = _cursor(connection, dictionary)
        try:
            current.execute('SELECT 7 AS value UNION ALL SELECT 8')
            expected = ({'value': 7}, {'value': 8}) if dictionary else ((7,), (8,))
            assert current.fetchall() == expected
            assert current.row_count == current.batch_row_count == 2
            columns = current.columns
            connection.close()
            index = _SlowIndex(0)
            assert current.scroll(index) is None
            assert index.calls == 1 and index.elapsed >= _INDEX_SECONDS
            assert current.fetchone() is None
            assert current.columns is columns
            assert current.row_count == current.batch_row_count == 2
        finally:
            current.close()


def _quote(name: str) -> str:
    return '`' + name.replace('`', '``') + '`'


@pytest.fixture(scope='module', params=['84', '97'])
def conditional_routine(request: pytest.FixtureRequest) -> Iterator[tuple[str, str]]:
    """Create only one uniquely named procedure on the selected owned server."""
    label = str(request.param)
    name = 'a_review_' + uuid4().hex[:16]
    with live(label) as owner, owner.cursor() as setup:
        setup.execute('SELECT DATABASE()')
        row = setup.fetchone()
        assert row is not None and isinstance(row[0], str)
        qualified = _quote(row[0]) + '.' + _quote(name)
        wide = ','.join(f'{value} AS c{value}' for value in range(256))
        setup.execute(
            f'CREATE PROCEDURE {qualified}(IN show_rows INT) '
            f'BEGIN IF show_rows THEN SELECT {wide}; END IF; END'
        )
        try:
            # Independent real-server controls establish each branch before
            # the quota-constrained SS call. The false branch is exactly one
            # no-row result, so its first End is the command's final ACK.
            assert setup.execute(f'CALL {qualified}(0)') == 0
            assert setup.description is None and setup.row_count == 0
            assert setup.nextset() is None
            assert setup.execute(f'CALL {qualified}(1)') == 1
            assert setup.fetchall() == (tuple(range(256)),)
            assert setup.nextset() is True
            assert setup.description is None and setup.row_count == 0
            assert setup.nextset() is None
            yield label, qualified
        finally:
            setup.execute(f'DROP PROCEDURE {qualified}')


@requires_mysql
@pytest.mark.parametrize('dictionary', [False, True])
@pytest.mark.parametrize('final_has_rows', [False, True], ids=['terminal-only', 'unread-rows'])
def test_prefix_publication_failure_uses_the_actual_final_stream_completion(
    conditional_routine: tuple[str, str], dictionary: bool, final_has_rows: bool,
) -> None:
    label, qualified = conditional_routine
    with live(label, result_memory_limit=_READER_LIMIT,
              total_result_memory_limit=4 * 1024 * 1024) as connection:
        current = _cursor(connection, dictionary)
        try:
            with pytest.raises(OperationalError) as caught:
                current.executemany(f'CALL {qualified}(%s)', [(1,), (int(final_has_rows),)])
            assert caught.value.code == SQLAeroErrorCode.RESOURCE_LIMIT
            assert caught.value.is_server_error is False
            assert 'native result requires' in caught.value.message
            assert str(_READER_LIMIT) in caught.value.message
            if not final_has_rows:
                # First request contributes its one row; the real final ACK
                # contributes zero. A local publication failure changes neither.
                assert current.batch_row_count == 1
            for _ in range(2):
                with pytest.raises(SQLAeroError) as boundary:
                    current.nextset()
                assert boundary.value.code == caught.value.code
                assert boundary.value.is_server_error is False
            assert connection.closed is final_has_rows
            if not final_has_rows:
                assert connection.ping() is True
        finally:
            current.close()
