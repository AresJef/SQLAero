"""Section 25 native quotas preserve recoverable streaming delivery positions."""

import pytest
from sqlaero import Connection, SSCursor, SSDictCursor
from sqlaero.errors import OperationalError, SQLAeroErrorCode
from test_public_connection import live, requires_mysql


ROWS = 128
PAYLOAD_BYTES = 4096
RESULT_LIMIT = 64 * 1024
PACKET_LIMIT = 16 * 1024
StreamingCursor = SSCursor | SSDictCursor
Row = tuple[object, ...] | dict[str, object]


def start_stream(connection: Connection, dictionary: bool, rows: int = ROWS) -> StreamingCursor:
    """Start one response whose individual rows fit the independent limits."""
    with connection.cursor() as setup:
        setup.execute("SET @a_resource_visits=0")
    cursor = connection.cursor(dict, streaming=True) if dictionary else connection.cursor(streaming=True)
    cursor.execute(
        f"WITH RECURSIVE seq(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM seq WHERE n<{rows}) "
        f"SELECT n AS ordinal,REPEAT('x',{PAYLOAD_BYTES}) AS payload,"
        "(@a_resource_visits:=@a_resource_visits+1) AS visits FROM seq ORDER BY n"
    )
    return cursor


def expected_row(ordinal: int, dictionary: bool) -> Row:
    """Use literal server values, independent of native buffering decisions."""
    payload = "x" * PAYLOAD_BYTES
    if dictionary:
        return {"ordinal": ordinal, "payload": payload, "visits": ordinal}
    return (ordinal, payload, ordinal)


def consume_small_batches(cursor: StreamingCursor, dictionary: bool, rows: int = ROWS) -> None:
    """Check every original row rather than accepting a prefix or replay."""
    for ordinal in range(1, rows + 1):
        assert cursor.fetchmany(1) == (expected_row(ordinal, dictionary),)
    assert cursor.fetchmany(1) == ()
    assert cursor.row_count == cursor.batch_row_count == rows


def assert_one_execution(connection: Connection, rows: int = ROWS) -> None:
    """Read the completed response's own session counter without rerunning it."""
    with connection.cursor() as observer:
        observer.execute("SELECT @a_resource_visits")
        assert observer.fetchone() == (rows,)


def assert_resource_error(error: OperationalError, limit: int) -> None:
    """Configured native quotas are local resource errors with real bounds."""
    assert type(error) is OperationalError
    assert error.code == SQLAeroErrorCode.RESOURCE_LIMIT
    assert error.is_server_error is False
    assert str(limit) in error.message


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("dictionary", [False, True])
def test_small_batches_fit_while_the_same_total_response_exceeds_the_result_limit(
    label: str, dictionary: bool,
) -> None:
    with live(label, result_memory_limit=RESULT_LIMIT,
              total_result_memory_limit=512 * 1024, max_received_packet=PACKET_LIMIT) as connection:
        cursor = start_stream(connection, dictionary)
        columns = cursor.columns
        consume_small_batches(cursor, dictionary)
        assert cursor.columns is columns and columns.names == ("ordinal", "payload", "visits")
        assert_one_execution(connection)
        cursor.close()


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("dictionary", [False, True])
@pytest.mark.parametrize("operation", ["fetchall", "fetchmany"])
def test_native_accumulation_failure_keeps_the_original_start_for_explicit_smaller_fetch(
    label: str, dictionary: bool, operation: str,
) -> None:
    connection = live(label, result_memory_limit=RESULT_LIMIT,
                      total_result_memory_limit=512 * 1024, max_received_packet=PACKET_LIMIT)
    try:
        cursor = start_stream(connection, dictionary)
        columns = cursor.columns
        with pytest.raises(OperationalError) as failure:
            if operation == "fetchall":
                cursor.fetchall()
            else:
                cursor.fetchmany(ROWS)
        assert_resource_error(failure.value, RESULT_LIMIT)
        assert connection.closed is False, "individually fitting rows must retain a safe continuation"
        assert cursor.columns is columns
        assert cursor.scroll(0, mode="absolute") is None
        consume_small_batches(cursor, dictionary)
        assert_one_execution(connection)
        cursor.close()
    finally:
        connection.close()


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("dictionary", [False, True])
def test_aggregate_pressure_keeps_the_other_result_until_its_owner_releases_it(
    label: str, dictionary: bool,
) -> None:
    aggregate_limit = 2 * 1024 * 1024
    aggregate_rows = ROWS
    # First prove the full response fits without another result owner. Its
    # own limit is deliberately higher so only the aggregate can bind here.
    with live(label, result_memory_limit=4 * 1024 * 1024,
              total_result_memory_limit=aggregate_limit, max_received_packet=1024 * 1024) as reference:
        baseline = start_stream(reference, dictionary, aggregate_rows)
        assert baseline.fetchall() == tuple(expected_row(index, dictionary)
                                            for index in range(1, aggregate_rows + 1))
        assert baseline.row_count == aggregate_rows
        assert_one_execution(reference, aggregate_rows)
        baseline.close()
    connection = live(label, result_memory_limit=4 * 1024 * 1024,
                      total_result_memory_limit=aggregate_limit, max_received_packet=1024 * 1024)
    try:
        held = connection.cursor()
        held.execute("SELECT REPEAT('h',524288) AS held")
        held_columns = held.columns
        second_held = connection.cursor()
        second_held.execute("SELECT REPEAT('i',524288) AS second_held")
        second_columns = second_held.columns
        cursor = start_stream(connection, dictionary, aggregate_rows)
        columns = cursor.columns
        with pytest.raises(OperationalError) as failure:
            cursor.fetchall()
        assert_resource_error(failure.value, aggregate_limit)
        assert "total result memory limit" in failure.value.message
        assert held.columns is held_columns and held.fetchone() == ("h" * 524288,)
        assert second_held.columns is second_columns and second_held.fetchone() == ("i" * 524288,)
        assert connection.closed is False, "aggregate pressure must preserve the bounded stream state"
        held.close()
        second_held.close()
        assert held_columns.names == ("held",) and second_columns.names == ("second_held",)
        assert cursor.columns is columns and cursor.scroll(0, mode="absolute") is None
        consume_small_batches(cursor, dictionary, aggregate_rows)
        assert_one_execution(connection, aggregate_rows)
        cursor.close()
    finally:
        connection.close()


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("dictionary", [False, True])
def test_one_logical_message_over_the_packet_limit_cannot_resume_as_smaller_fetches(
    label: str, dictionary: bool,
) -> None:
    connection = live(label, max_received_packet=PACKET_LIMIT,
                      result_memory_limit=512 * 1024, total_result_memory_limit=1024 * 1024)
    try:
        cursor = connection.cursor(dict, streaming=True) if dictionary else connection.cursor(streaming=True)
        cursor.execute("SELECT 1 AS ordinal,REPEAT('z',32768) AS payload")
        columns = cursor.columns
        assert connection.max_received_packet == PACKET_LIMIT
        with pytest.raises(OperationalError) as failure:
            cursor.fetchone()
        assert_resource_error(failure.value, PACKET_LIMIT)
        assert "logical message" in failure.value.message
        assert connection.closed and cursor.row_count is None and cursor.batch_row_count is None
        assert cursor.columns is columns and columns.names == ("ordinal", "payload")
        with pytest.raises(OperationalError) as repeated:
            cursor.fetchmany(1)
        assert repeated.value.code == failure.value.code
    finally:
        connection.close()
