"""Every public interval class uses one native expression writer."""

from collections.abc import Callable
from datetime import date, datetime, timedelta
from inspect import signature

import pytest
from sqlaero import Connection, sqlfunc, sqlintv
from sqlaero.errors import DataError
from sqlaero.typing import EscapeInput

from test_public_connection import connection as connection, requires_mysql


SINGLE_UNITS = (
    ("YEAR", "years"), ("QUARTER", "quarters"), ("MONTH", "months"),
    ("WEEK", "weeks"), ("DAY", "days"), ("HOUR", "hours"),
    ("MINUTE", "minutes"), ("SECOND", "seconds"), ("MICROSECOND", "microseconds"),
)

COMPOSITE_UNITS: tuple[tuple[str, tuple[str, ...], tuple[int, ...], str, timedelta], ...] = (
    ("DAY_HOUR", ("days", "hours"), (1, -2), "0 22", timedelta(days=1, hours=-2)),
    ("DAY_MINUTE", ("days", "hours", "minutes"), (0, -2, 30), "-0 01:30",
     timedelta(hours=-2, minutes=30)),
    ("DAY_SECOND", ("days", "hours", "minutes", "seconds"), (1, 26, 70, 80),
     "2 03:11:20", timedelta(days=1, hours=26, minutes=70, seconds=80)),
    ("DAY_MICROSECOND", ("days", "hours", "minutes", "seconds", "microseconds"),
     (0, 0, 0, 0, -1), "-0 00:00:00.000001", timedelta(microseconds=-1)),
    ("HOUR_MINUTE", ("hours", "minutes"), (1, -60), "0:00", timedelta()),
    ("HOUR_SECOND", ("hours", "minutes", "seconds"), (1, -1, 2), "0:59:02",
     timedelta(hours=1, minutes=-1, seconds=2)),
    ("HOUR_MICROSECOND", ("hours", "minutes", "seconds", "microseconds"),
     (0, 1, -60, 2), "0:00:00.000002", timedelta(microseconds=2)),
    ("MINUTE_SECOND", ("minutes", "seconds"), (0, -61), "-1:01", timedelta(seconds=-61)),
    ("MINUTE_MICROSECOND", ("minutes", "seconds", "microseconds"), (0, 1, -1),
     "0:00.999999", timedelta(seconds=1, microseconds=-1)),
    ("SECOND_MICROSECOND", ("seconds", "microseconds"), (-2, 30), "-1.999970",
     timedelta(seconds=-2, microseconds=30)),
)


@pytest.mark.parametrize(("unit", "parameter"), SINGLE_UNITS)
def test_single_interval_name_and_signature(unit: str, parameter: str) -> None:
    constructor: type[object] = getattr(sqlintv, unit)
    assert unit in sqlintv.__all__
    assert constructor.__module__ == "sqlaero.sqlintv"
    assert constructor.__name__ == unit
    assert constructor.__base__ is sqlintv.DAY.__base__
    assert list(signature(constructor).parameters) == [parameter]
    with pytest.raises(TypeError):
        constructor(1, 2)  # type: ignore[call-arg]


@requires_mysql
@pytest.mark.parametrize(("unit", "_parameter"), SINGLE_UNITS)
def test_single_interval_executes_against_fixed_sql(
    connection: Connection, unit: str, _parameter: str,
) -> None:
    constructor: Callable[[object], EscapeInput] = getattr(sqlintv, unit)
    count = sqlfunc.ABS(-2)
    interval = constructor(count)
    assert connection.escape(interval) == f"INTERVAL ABS(-2) {unit}"
    with connection.cursor() as cursor:
        cursor.execute(
            f"SELECT DATE_ADD(%s, %s), DATE_ADD(%s, INTERVAL 2 {unit})",
            (datetime(2026, 11, 27, 12), interval, datetime(2026, 11, 27, 12)),
        )
        result = cursor.fetchone()
        assert result is not None and result[0] == result[1] and result[0] is not None


@requires_mysql
@pytest.mark.parametrize(("unit", "parameters", "args", "literal", "delta"), COMPOSITE_UNITS)
def test_compound_time_intervals_reduce_to_one_operation(
    connection: Connection, unit: str, parameters: tuple[str, ...], args: tuple[int, ...],
    literal: str, delta: timedelta,
) -> None:
    constructor: Callable[..., EscapeInput] = getattr(sqlintv, unit)
    assert unit in sqlintv.__all__
    assert tuple(signature(constructor).parameters) == parameters
    start = datetime(2026, 11, 1, 12)
    interval = constructor(*args)
    assert connection.escape(interval) == f"INTERVAL '{literal}' {unit}"
    with connection.cursor() as cursor:
        cursor.execute("SELECT DATE_ADD(CAST(%s AS DATETIME(6)), %s)", (start, interval))
        assert cursor.fetchone() == (start + delta,)
    with pytest.raises(TypeError):
        constructor(*args[:-1])
    invalid: tuple[object, ...] = (True, 1.5, "1", sqlfunc.ABS(1))
    for value in invalid:
        with pytest.raises(TypeError):
            constructor(*args[:-1], value)


@requires_mysql
@pytest.mark.parametrize(("start", "args", "literal", "result"), (
    (date(2023, 4, 30), (1, -2), "0-10", date(2024, 2, 29)),
    (date(2024, 12, 31), (-1, 2), "-0-10", date(2024, 2, 29)),
    (date(2024, 3, 31), (0, -1), "-0-01", date(2024, 2, 29)),
    (date(2025, 3, 31), (0, -1), "-0-01", date(2025, 2, 28)),
    (date(2024, 2, 29), (1, -12), "0-00", date(2024, 2, 29)),
    (date(2024, 2, 29), (1, 1), "1-01", date(2025, 3, 29)),
))
def test_compound_calendar_intervals_apply_the_net_months_once(
    connection: Connection, start: date, args: tuple[int, int], literal: str, result: date,
) -> None:
    interval = sqlintv.YEAR_MONTH(*args)
    assert tuple(signature(sqlintv.YEAR_MONTH).parameters) == ("years", "months")
    assert sqlintv.YEAR_MONTH.__base__ is sqlintv.DAY.__base__
    assert connection.escape(interval) == f"INTERVAL '{literal}' YEAR_MONTH"
    with connection.cursor() as cursor:
        cursor.execute("SELECT %s", sqlfunc.DATE_ADD(sqlfunc.DATE(start), interval))
        assert cursor.fetchone() == (result,)


@requires_mysql
@pytest.mark.parametrize(("args", "literal"), (
    ((0, -2, 30, 0), "-0 01:30:00"),
    ((0, -2, -30, 0), "-0 02:30:00"),
    ((1, -24, 0, 0), "0 00:00:00"),
    ((1, 26, 70, 80), "2 03:11:20"),
    ((-1, 2, 0, 0), "-0 22:00:00"),
    ((0, 0, 0, -1), "-0 00:00:01"),
))
def test_day_second_uses_the_net_sign(
    connection: Connection, args: tuple[int, int, int, int], literal: str,
) -> None:
    days, hours, minutes, seconds = args
    interval = sqlintv.DAY_SECOND(*args)
    start = datetime(2026, 11, 1, 12)
    expected = start + timedelta(days=days, hours=hours, minutes=minutes, seconds=seconds)
    assert connection.escape(interval) == f"INTERVAL '{literal}' DAY_SECOND"
    with connection.cursor() as cursor:
        cursor.execute("SELECT DATE_ADD(CAST(%s AS DATETIME(6)), %s)", (start, interval))
        assert cursor.fetchone() == (expected,)


@requires_mysql
def test_compound_negative_and_month_end_server_controls(connection: Connection) -> None:
    with connection.cursor() as cursor:
        cursor.execute(
            "SELECT DATE_ADD(TIMESTAMP '2026-11-01 12:00:00', INTERVAL '-1 02:00:00' DAY_SECOND), "
            "DATE_ADD(DATE '2024-12-31', INTERVAL '-1-02' YEAR_MONTH), "
            "DATE_ADD(DATE_ADD(DATE '2024-02-29', INTERVAL 1 YEAR), INTERVAL -12 MONTH)"
        )
        assert cursor.fetchone() == (
            datetime(2026, 10, 31, 10), date(2023, 10, 31), date(2024, 2, 28),
        )


@requires_mysql
def test_compound_interval_cancellation_and_overflow(connection: Connection) -> None:
    class Integer(int):
        def __index__(self) -> int:
            raise AssertionError("use the stored integer")

        def __int__(self) -> int:
            raise AssertionError("use the stored integer")

    huge = 1 << 10000
    cancel = sqlintv.YEAR_MONTH(Integer(huge), -12 * huge + 1)
    assert connection.escape(cancel) == "INTERVAL '0-01' YEAR_MONTH"
    with pytest.raises(DataError, match="YEAR_MONTH.*range"):
        connection.escape(sqlintv.YEAR_MONTH(huge, 0))
