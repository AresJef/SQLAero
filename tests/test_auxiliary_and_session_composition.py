"""Public auxiliary projections retain their own session and operation context."""

from concurrent.futures import ThreadPoolExecutor
from collections.abc import Callable
from threading import Event
from types import FrameType
from typing import Any
from uuid import uuid4

import pytest
from _native_signal import NativeAlarm, native_call_offsets
from sqlaero import Connection
from sqlaero.errors import OperationalTimeoutError
from test_public_connection import live, requires_mysql


KINDS = ("tuple", "dict", "stream_tuple", "stream_dict")
VALUE = "é'\\"
OLD_LITERAL = "'é\\'\\\\'"
NEW_LITERAL = "'é''\\'"


def public_cursor(connection: Connection, kind: str, *, prepared: bool = False) -> Any:
    """Select only an existing public Cursor family."""
    if kind.endswith("dict"):
        return connection.cursor(dict, streaming=kind.startswith("stream"), prepared=prepared)
    return connection.cursor(streaming=kind.startswith("stream"), prepared=prepared)


def public_row(kind: str, value: int) -> tuple[int] | dict[str, int]:
    return {"v": value} if kind.endswith("dict") else (value,)


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("kind", KINDS)
def test_auxiliary_snapshot_allows_fetch_sql_and_another_projection_during_conversion(
    label: str, kind: str,
) -> None:
    entered, release = Event(), Event()

    class PausedInt(int):
        def __int__(self) -> int:
            entered.set()
            assert release.wait(5), "the observer did not release input conversion"
            return 7

    with live(label) as connection, public_cursor(connection, kind) as cursor:
        with connection.cursor() as setup:
            setup.execute("SET NAMES utf8mb4; SET SESSION sql_mode=''")
        cursor.execute("SELECT 41 AS v UNION ALL SELECT 42")
        columns, description = cursor.columns, cursor.description
        assert connection.escape(VALUE) == OLD_LITERAL
        with ThreadPoolExecutor(max_workers=1) as executor:
            pending = executor.submit(cursor.mogrify, "SELECT %s,%s", (PausedInt(7), VALUE))
            try:
                assert entered.wait(5), "mogrify did not enter the supported int protocol"
                assert connection.escape(VALUE) == OLD_LITERAL
                assert cursor.fetchone() == public_row(kind, 41)
                assert cursor.fetchall() == (public_row(kind, 42),)
                with connection.cursor() as control:
                    control.execute("SET NAMES latin1; SET SESSION sql_mode='NO_BACKSLASH_ESCAPES'")
                assert connection.escape(VALUE) == NEW_LITERAL
                assert cursor.mogrify("SELECT %s", VALUE) == "SELECT " + NEW_LITERAL
                assert cursor.columns is columns and cursor.description == description
                assert cursor.row_count == 2 and cursor.fetchall() == ()
            finally:
                release.set()
            assert pending.result(timeout=5) == "SELECT 7," + OLD_LITERAL
        assert cursor.mogrify("SELECT %s", VALUE) == "SELECT " + NEW_LITERAL


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("finish", ["healthy", "close", "timeout"])
def test_auxiliary_input_failure_does_not_claim_sql_or_destroy_the_saved_reference(
    label: str, finish: str,
) -> None:
    entered, release = Event(), Event()
    original = MemoryError("one supported auxiliary input protocol failed")

    class FailedInt(int):
        def __int__(self) -> int:
            entered.set()
            assert release.wait(5), "the observer did not release failed input conversion"
            raise original

    with live(label) as connection, connection.cursor() as business:
        business.execute("SELECT 41 AS v UNION ALL SELECT 42")
        columns, description = business.columns, business.description
        with ThreadPoolExecutor(max_workers=1) as executor:
            pending = executor.submit(connection.escape, FailedInt(7))
            try:
                assert entered.wait(5)
                if finish == "close":
                    connection.close()
                elif finish == "timeout":
                    connection.operation_timeout = 0.02
                    with connection.cursor() as command:
                        with pytest.raises(OperationalTimeoutError):
                            command.execute("SELECT SLEEP(0.2)")
                    assert connection.closed
                else:
                    with connection.cursor() as command:
                        assert command.execute("SELECT 55") == 1
                        assert command.fetchone() == (55,)
                assert connection.escape(9) == "9"
                assert business.mogrify("SELECT %s", 10) == "SELECT 10"
                assert business.columns is columns and business.description == description
                assert business.fetchone() == (41,)
            finally:
                release.set()
            with pytest.raises(MemoryError) as failure:
                pending.result(timeout=5)
            assert failure.value is original
        assert business.fetchone() == (42,) and business.fetchall() == ()
        assert connection.escape(11) == "11"
        assert connection.closed is (finish != "healthy")


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
def test_actual_signal_interrupts_only_the_auxiliary_conversion(
    label: str, record_property: Callable[[str, object], None],
) -> None:
    import time

    original = KeyboardInterrupt("actual auxiliary OS signal")
    source = "x'\\" * (4 * 1024 * 1024)
    with live(label) as connection, connection.cursor(streaming=True) as business:
        business.execute("SELECT 41 AS v UNION ALL SELECT 42")
        columns = business.columns
        deliveries = 0
        converted = False
        completed = 0
        observed: list[int] = []

        class AdaptedInt(int):
            def __int__(self) -> int:
                nonlocal converted
                converted = True
                return 7

        parameters = (AdaptedInt(7), source)
        # A fixed finite exposure follows the existing cold TLS-display probe;
        # this is not an assumed 2ms OS-dispatch SLA or an unbounded retry.
        attempts = range(64)

        def convert() -> None:
            nonlocal converted, completed
            for _ in attempts:
                converted = False
                connection.escape(parameters)
                completed += 1

        calls = native_call_offsets(convert.__code__)

        def interrupt(_number: int, frame: FrameType | None) -> None:
            nonlocal deliveries
            deliveries += 1
            # The Python input callback must already have returned. Count only
            # the actual native escape CALL, not readiness or later SQL/fetch.
            if (not converted or frame is None or frame.f_code is not convert.__code__
                    or frame.f_lasti not in calls):
                return
            observed.append(frame.f_lasti)
            alarm.cancel()
            raise original

        alarm = NativeAlarm(interrupt)
        try:
            alarm.start(0.0001, 0.0001, wait_ready=True)
            deadline = time.monotonic() + 5
            while deliveries == 0:
                assert time.monotonic() < deadline, 'real OS source did not deliver its readiness control'
                time.sleep(0.001)
            ready_deliveries = deliveries
            with pytest.raises(KeyboardInterrupt) as failure:
                convert()
            assert failure.value is original
        finally:
            alarm.close()
        assert len(observed) == 1 and completed < len(attempts)
        assert not hasattr(original, 'error_info')
        record_property('native_call_offset_at_signal', observed)
        record_property('real_deliveries_before_auxiliary_work', ready_deliveries)
        record_property('fixed_auxiliary_exposure_calls', len(attempts))
        record_property('completed_auxiliary_calls_before_signal', completed)
        assert not connection.closed and business.columns is columns
        assert business.fetchall() == ((41,), (42,))
        assert connection.escape(12) == "12"
        with connection.cursor() as command:
            assert command.execute("SELECT 56") == 1
            assert command.fetchone() == (56,)


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("kind", KINDS)
def test_public_multigroup_values_fixed_tail_merges_and_rolls_over_once(
    label: str, kind: str,
) -> None:
    payloads = tuple(chr(65 + index) * 150 for index in range(6))
    source = [(1, payloads[0], 2, payloads[1]), (3, payloads[2], 4, payloads[3]),
              (5, payloads[4], 6, payloads[5])]
    prefix = "INSERT a_composed_values(id,v) VALUES "
    suffix = " AS new ON DUPLICATE KEY UPDATE v=CONCAT(new.v,'!')"
    sql = prefix + "(%s,%s),(%s,%s)" + suffix
    groups = [f"({a},'{b}'),({c},'{d}')" for a, b, c, d in source]
    expected = [prefix + ",".join(groups[:2]) + suffix, prefix + groups[2] + suffix]
    with live(label, max_allowed_packet="1KB") as connection:
        with connection.cursor() as setup:
            setup.execute("CREATE TEMPORARY TABLE a_composed_values(id INT PRIMARY KEY,v VARCHAR(200))")
            setup.execute("INSERT a_composed_values VALUES (1,'old')")
        assert all(1 + len(request.encode()) <= connection.max_allowed_packet for request in expected)
        assert 1 + len((prefix + ",".join(groups) + suffix).encode()) > connection.max_allowed_packet
        with public_cursor(connection, kind) as cursor:
            assert cursor.mogrify(sql, source, many=True) == expected
            assert cursor.execute(sql, source, many=True) == 7
            assert cursor.batch_row_count == 7
            counts: list[int] = []
            while True:
                count = cursor.row_count
                assert type(count) is int and count > 0
                assert cursor.rowcount == cursor.affected_rows == count
                assert cursor.warning_count == 0 and cursor.description is None
                counts.append(count)
                if not cursor.nextset():
                    break
                assert cursor.batch_row_count == 7
            assert counts == [5, 2] and sum(counts) == cursor.batch_row_count
        with connection.cursor() as verify:
            verify.execute("SELECT id,v FROM a_composed_values ORDER BY id")
            assert verify.fetchall() == tuple((index + 1, value + ("!" if index == 0 else ""))
                                              for index, value in enumerate(payloads))


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
def test_init_single_units_discard_multisql_and_call_sets_without_ending_the_transaction(
    label: str,
) -> None:
    name = "a_init_composition_" + uuid4().hex
    qualified = "`sqlaero_test`.`" + name + "`"
    with live(label) as setup, setup.cursor() as create:
        create.execute(f"CREATE PROCEDURE {qualified}() BEGIN "
                       "SET @a_init_composed_calls=@a_init_composed_calls+1; "
                       "SELECT 10 AS first_call_set; INSERT a_init_composed VALUES (7); "
                       "SELECT 11 AS second_call_set; END")
        try:
            units = (
                "SET @a_init_composed_calls=0; SET SESSION autocommit=0; "
                "CREATE TEMPORARY TABLE a_init_composed(v INT); "
                f"INSERT a_init_composed VALUES (1); SELECT 20 AS first_unit_set; CALL {qualified}(); "
                "SELECT 21 AS last_unit_set",
                "INSERT a_init_composed VALUES (2); SET SESSION wait_timeout=333; SELECT 22 AS final_unit_set",
            )
            with live(label, init_sql=units, operation_timeout=1e-9) as connection:
                connection.operation_timeout = None
                assert connection.autocommit is False and connection.wait_timeout == 333
                with connection.cursor() as verify:
                    verify.execute("SELECT v FROM a_init_composed ORDER BY v")
                    assert verify.fetchall() == ((1,), (2,), (7,))
                    verify.execute("SELECT @a_init_composed_calls,@@session.time_zone")
                    assert verify.fetchone() == (1, "+00:00")
                    connection.rollback()
                    verify.execute("SELECT v FROM a_init_composed")
                    assert verify.fetchall() == ()
        finally:
            create.execute(f"DROP PROCEDURE IF EXISTS {qualified}")


def statement_counts(connection: Connection) -> dict[str, int]:
    """Read only public, fixed statement counters to prove local refusal."""
    result: dict[str, int] = {}
    with connection.cursor() as cursor:
        cursor.execute("SHOW SESSION STATUS WHERE Variable_name IN ('Com_stmt_prepare','Com_stmt_execute')")
        for name, value in cursor.fetchall():
            assert isinstance(name, str) and isinstance(value, str) and value.isdecimal()
            result[name] = int(value)
    assert set(result) == {"Com_stmt_prepare", "Com_stmt_execute"}
    return result


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
def test_runtime_mb3_refreshes_prepared_and_auxiliary_context_but_keeps_old_results(
    label: str,
) -> None:
    sql = "SELECT %s AS `é`,COLLATION(%s) AS codec UNION ALL SELECT %s,COLLATION(%s)"
    with live(label) as connection, connection.cursor() as old, connection.cursor(prepared=True) as prepared:
        old.execute("SELECT %s AS `é` UNION ALL SELECT %s", ("😀", "\U00010000"))
        columns, description = old.columns, old.description
        prepared.execute(sql, ("😀", "😀", "\U00010000", "\U00010000"))
        assert prepared.fetchall() == (("😀", "utf8mb4_0900_ai_ci"), ("\U00010000", "utf8mb4_0900_ai_ci"))
        with connection.cursor() as control:
            control.execute("SET NAMES utf8mb3 COLLATE utf8mb3_bin")
        assert connection.escape("中文\uffff") == "'中文\uffff'"
        assert prepared.mogrify("SELECT %s", "中文\uffff") == "SELECT '中文\uffff'"
        prepared.execute(sql, ("中文", "中文", "\uffff", "\uffff"))
        assert prepared.fetchone() == ("中文", "utf8mb3_bin")
        current_columns = prepared.columns
        before = statement_counts(connection)
        source = "中😀"
        for operation in (lambda: connection.escape(source), lambda: prepared.mogrify("SELECT %s", source),
                          lambda: prepared.execute("SELECT %s", source)):
            with pytest.raises(UnicodeEncodeError) as failure:
                operation()
            assert failure.value.object is source
            assert (failure.value.encoding, failure.value.start, failure.value.end) == ("utf8mb3", 1, 2)
        template = "SELECT '😀'"
        with pytest.raises(UnicodeEncodeError) as failure:
            prepared.execute(template)
        assert failure.value.start == template.index("😀")
        assert statement_counts(connection) == before
        assert prepared.columns is current_columns and prepared.fetchone() == ("\uffff", "utf8mb3_bin")
        assert old.columns is columns and old.description == description and columns.names == ("é",)
        assert old.fetchall() == (("😀",), ("\U00010000",))
        with connection.cursor() as control:
            control.execute("SET NAMES utf8mb4")
        assert connection.escape("😀") == "'😀'"
        assert old.mogrify("SELECT %s", "😀") == "SELECT '😀'"
