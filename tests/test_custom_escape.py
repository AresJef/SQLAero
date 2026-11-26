"""Public CustomEscape callback and SQL placement behavior."""

from collections.abc import Callable
from concurrent.futures import ThreadPoolExecutor
from inspect import signature
from threading import Barrier
from uuid import uuid4

import pytest
from sqlaero import Connection, CustomEscape, JSON, json_dumps, sqlfunc, sqlintv
from sqlaero.errors import InterfaceBusyError, InterfaceError, NotSupportedError, ProgrammingError

from test_feature_expansion_a_planner import plan
from test_prepared_cursor import statement_counts
from test_public_connection import connection as connection, live, requires_mysql


def test_custom_escape_constructor_keeps_read_only_references() -> None:
    values = [7]
    calls: list[list[int]] = []

    def convert(value: list[int]) -> str:
        calls.append(value)
        return str(value[0])

    wrapped = CustomEscape(values, convert)
    assert wrapped.value is values and wrapped.converter is convert
    assert list(signature(CustomEscape).parameters) == ["value", "converter"]
    assert repr(wrapped) and str(wrapped)
    assert calls == []
    with pytest.raises(AttributeError):
        setattr(wrapped, "value", [])
    with pytest.raises(AttributeError):
        setattr(wrapped, "converter", convert)
    with pytest.raises(TypeError, match="callable"):
        CustomEscape(values, 1)  # type: ignore[arg-type]


@requires_mysql
def test_each_position_uses_the_original_value_once(connection: Connection) -> None:
    values = [-7]
    calls: list[list[int]] = []

    def convert(value: list[int]) -> str:
        assert value is values
        calls.append(value)
        return str(value[0])

    wrapped = CustomEscape(values, convert)
    with connection.cursor() as cursor:
        args = (wrapped, sqlfunc.ABS(wrapped))
        assert cursor.mogrify("SELECT %s,%s", args) == "SELECT -7,ABS(-7)"
        assert len(calls) == 2
        values[0] = -19
        cursor.execute("SELECT %s,%s", args)
        assert cursor.fetchone() == (-19, 19)
        assert len(calls) == 4


@requires_mysql
def test_custom_preserves_empty_whitespace_markers_and_original_error(connection: Connection) -> None:
    for text in ("", "  \t ", "  '\x25s'  "):
        wrapped = CustomEscape(text, lambda value: value)
        assert connection.escape(wrapped) == text
        with connection.cursor() as cursor:
            assert cursor.mogrify("SELECT 0%s", wrapped) == "SELECT 0" + text

    original = RuntimeError("from converter")

    def fail(_value: int) -> str:
        raise original

    with pytest.raises(RuntimeError) as caught:
        connection.escape(CustomEscape(1, fail))
    assert caught.value is original


@requires_mysql
def test_custom_accepts_only_exact_strings(connection: Connection) -> None:
    class Text(str):
        def __str__(self) -> str:
            raise AssertionError("do not coerce the converter result")

    for result in (Text("7"), b"7", None, CustomEscape(7, str)):
        convert: Callable[[int], object] = lambda _value: result
        with pytest.raises(TypeError, match="exact.*str"):
            connection.escape(CustomEscape(1, convert))  # type: ignore[arg-type]


@requires_mysql
def test_prepared_custom_preview_and_json_never_cross_execution_boundaries(connection: Connection) -> None:
    calls: list[int] = []

    def convert(value: int) -> str:
        calls.append(value)
        return str(value)

    wrapped = CustomEscape(7, convert)
    with connection.cursor(prepared=True) as cursor:
        before = statement_counts(connection)
        with pytest.raises(NotSupportedError):
            cursor.execute("SELECT %s", wrapped)
        assert statement_counts(connection) == before
        assert not calls
        assert cursor.mogrify("SELECT %s", wrapped) == "SELECT 7"
        assert statement_counts(connection) == before
    assert calls == [7]
    with pytest.raises(TypeError):
        sqlintv.DAY_SECOND(0, 0, 0, wrapped)  # type: ignore[arg-type]
    with pytest.raises(TypeError, match="CustomEscape"):
        json_dumps({"x": wrapped})
    with pytest.raises(TypeError, match="CustomEscape"):
        connection.escape(JSON({"x": wrapped}))
    assert calls == [7]


@requires_mysql
@pytest.mark.parametrize("text", ("1; SELECT 2", "'unclosed", "1 /*", "1 # unfinished", "1)"))
def test_custom_fragment_rejects_independent_lexical_boundary_errors(
    connection: Connection, text: str,
) -> None:
    wrapped = CustomEscape(text, lambda value: value)
    with connection.cursor() as cursor:
        with pytest.raises(ProgrammingError):
            cursor.mogrify("SELECT %s", wrapped)


@requires_mysql
@pytest.mark.parametrize("prepared", (False, True))
def test_fragment_cannot_join_a_fixed_token_or_change_later_context(
    connection: Connection, prepared: bool,
) -> None:
    with connection.cursor(prepared=prepared) as cursor:
        for sql, args in (
            ("SELECT /%s", (CustomEscape("* 7 */", lambda value: value),)),
            ("SELECT 1 %s 2", (CustomEscape("--", lambda value: value),)),
            ("SE%s sql_mode='ANSI_QUOTES'; SELECT %s",
             (CustomEscape("T", lambda value: value), "after")),
        ):
            with pytest.raises(ProgrammingError, match="boundary|context"):
                cursor.mogrify(sql, args)
    with connection.cursor() as cursor:
        assert cursor.mogrify("SELECT %s; SELECT %s", (CustomEscape(7, str), 8)) == "SELECT 7; SELECT 8"


@requires_mysql
def test_fragment_uses_confirmed_ansi_quotes_mode(connection: Connection) -> None:
    wrapped = CustomEscape('"a\\"b"', lambda value: value)
    with connection.cursor() as cursor:
        cursor.execute("SET SESSION sql_mode=''")
        cursor.execute("SELECT %s", wrapped)
        assert cursor.fetchone() == ('a"b',)
        cursor.execute("SET SESSION sql_mode='ANSI_QUOTES'")
        with pytest.raises(ProgrammingError, match="quote"):
            cursor.execute("SELECT %s", wrapped)


@requires_mysql
def test_fragment_uses_current_charset_in_bounded_windows(connection: Connection) -> None:
    text = "é" * (256 * 1024 + 17)
    with connection.cursor() as cursor:
        cursor.execute("SET NAMES latin1")
        fragment = CustomEscape(text, lambda value: "'" + value + "'")
        cursor.execute("SELECT CHAR_LENGTH(%s)", fragment)
        assert cursor.fetchone() == (len(text),)
        with pytest.raises(UnicodeEncodeError):
            cursor.execute("SELECT %s", CustomEscape("中", lambda value: "'" + value + "'"))


@requires_mysql
def test_safe_values_merge_and_a_top_level_separator_uses_single_rows(connection: Connection) -> None:
    seen: list[int] = []

    def convert(value: int) -> str:
        seen.append(value)
        return str(value)

    def inserts() -> int:
        result: tuple[object, ...] | None = None
        with connection.cursor() as status:
            status.execute("SHOW SESSION STATUS LIKE 'Com_insert'")
            result = status.fetchone()
        assert result is not None and isinstance(result[1], str)
        return int(result[1])

    with connection.cursor() as cursor:
        cursor.execute("CREATE TEMPORARY TABLE escape_merge (a INT, b INT)")
        before = inserts()
        rows = [(CustomEscape(value, convert), 0) for value in range(3)]
        assert cursor.executemany("INSERT INTO escape_merge VALUES (%s,%s)", rows) == 3
        assert inserts() - before == 1
        assert seen == [0, 1, 2]

        def two_values(value: int) -> str:
            seen.append(value)
            return f"{value}, {value + 1}"

        before = inserts()
        assert cursor.executemany(
            "INSERT INTO escape_merge VALUES (%s)",
            [CustomEscape(value, two_values) for value in (3, 4)],
        ) == 2
        assert inserts() - before == 2
        assert seen == [0, 1, 2, 3, 4]
        cursor.execute("SELECT a, b FROM escape_merge ORDER BY a")
        assert cursor.fetchall() == ((0, 0), (1, 0), (2, 0), (3, 4), (4, 5))


@requires_mysql
def test_custom_success_cannot_send_after_the_callback_closed_the_cursor(connection: Connection) -> None:
    cursor = connection.cursor()

    def close(value: int) -> str:
        cursor.close()
        return str(value)

    with pytest.raises(InterfaceError):
        cursor.execute("SELECT %s", CustomEscape(1, close))


def test_pending_row_reencodes_the_saved_converter_result() -> None:
    calls: list[str] = []

    def quote(value: str) -> str:
        calls.append(value)
        return "'" + value + "'"

    item = CustomEscape("é", quote)
    first = b"\x03INSERT t VALUES ('\xc3\xa9')"
    second = b"\x03INSERT t VALUES ('\xe9')"
    actual = plan(
        "INSERT t VALUES (%s)", [item, item], many=True,
        limit=len(first), change_charset="latin1",
    )
    assert actual == [(first, 0, 1, False), (second, 1, 2, True)]
    assert calls == ["é", "é"]


@requires_mysql
def test_custom_error_keeps_the_completed_prefix(connection: Connection) -> None:
    error = RuntimeError("second request was not built")
    visited: list[int] = []

    def convert(value: int) -> str:
        visited.append(value)
        if value == 2:
            raise error
        return str(value)

    with connection.cursor() as cursor:
        cursor.execute("CREATE TEMPORARY TABLE escape_prefix (n INT)")
        cursor.execute("INSERT INTO escape_prefix VALUES (0)")
        with pytest.raises(RuntimeError) as caught:
            cursor.executemany(
                "UPDATE escape_prefix SET n=%s",
                [CustomEscape(value, convert) for value in (1, 2, 3)],
            )
        assert caught.value is error
        assert visited == [1, 2]
        cursor.execute("SELECT n FROM escape_prefix")
        assert cursor.fetchone() == (1,)


@requires_mysql
def test_call_uses_custom_in_and_inout_and_requires_literal_none_for_out(connection: Connection) -> None:
    name = "sqlaero_ce_" + uuid4().hex
    calls: list[int] = []

    def convert(value: int) -> str:
        calls.append(value)
        return str(value)

    with connection.cursor() as cursor:
        cursor.execute(
            f"CREATE PROCEDURE {name}(IN a INT, OUT b INT, INOUT c INT) "
            "BEGIN SET b = a + c; SET c = c + 1; SELECT a; END"
        )
        try:
            modes = ("IN", "OUT", "INOUT")
            local = CustomEscape(0, lambda value: calls.append(value) or "NULL")
            with pytest.raises(ValueError, match="None"):
                cursor.callproc(name, (1, local, 2), parameter_modes=modes)
            assert calls == []
            with connection.cursor(prepared=True) as prepared:
                before = statement_counts(connection)
                with pytest.raises(NotSupportedError):
                    prepared.callproc(name, (CustomEscape(7, convert), None, 2), parameter_modes=modes)
                assert statement_counts(connection) == before
            assert calls == []
            cursor.callproc(name, (CustomEscape(7, convert), None, CustomEscape(2, convert)),
                            parameter_modes=modes)
            assert cursor.fetchone() == (7,)
            while cursor.nextset() is not None:
                pass
            outputs = cursor.call_output_params
            assert outputs is not None and dict(outputs) == {1: 9, 2: 3}
            assert calls == [7, 2]
        finally:
            cursor.execute(f"DROP PROCEDURE {name}")


@requires_mysql
@pytest.mark.parametrize("raises", (False, True))
def test_custom_callback_retiring_a_transaction_preserves_the_actual_state(
    connection: Connection, raises: bool,
) -> None:
    with connection.cursor() as setup:
        setup.execute("CREATE TEMPORARY TABLE escape_retire (n INT)")
    transaction = connection.transaction()
    current = transaction.__enter__()
    original = RuntimeError("callback retired its transaction")
    exited = False

    def convert(value: int) -> str:
        nonlocal exited
        try:
            transaction.__exit__(None, None, None)
        except InterfaceBusyError:
            pass
        finally:
            exited = True
        if raises:
            raise original
        return str(value)

    try:
        with pytest.raises(RuntimeError if raises else InterfaceError) as caught:
            current.execute("INSERT INTO escape_retire VALUES (%s)", CustomEscape(7, convert))
        assert exited and current.closed
        if raises:
            assert caught.value is original
        with connection.cursor() as check:
            check.execute("SELECT COUNT(*) FROM escape_retire")
            assert check.fetchone() == (0,)
    finally:
        if not exited:
            transaction.__exit__(None, None, None)
        connection.rollback()


@requires_mysql
@pytest.mark.parametrize("label", ("84", "97"))
def test_independent_connections_share_one_immutable_wrapper(label: str) -> None:
    workers = 4
    entered = Barrier(workers)

    def convert(value: int) -> str:
        entered.wait(timeout=5)
        return str(abs(value))

    wrapped = CustomEscape(-7, convert)

    def read() -> tuple[object, ...] | None:
        result = None
        with live(label) as separate, separate.cursor() as cursor:
            cursor.execute("SELECT %s, CONNECTION_ID()", wrapped)
            result = cursor.fetchone()
        return result

    with ThreadPoolExecutor(max_workers=workers) as pool:
        futures = tuple(pool.submit(read) for _ in range(workers))
        results = tuple(future.result(timeout=10) for future in futures)
    assert all(result is not None and result[0] == 7 for result in results)
    assert len({result[1] for result in results if result is not None}) == workers
