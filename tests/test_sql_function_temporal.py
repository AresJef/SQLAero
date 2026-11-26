"""Verify the temporal function catalogue against MySQL's native syntax."""

from collections.abc import Callable
from datetime import date, datetime, timedelta
from inspect import signature

import pytest
from sqlaero import Connection, CustomEscape, sqlfunc, sqlintv
from sqlaero.typing import EscapeInput

from test_public_connection import connection as connection, requires_mysql


CASES: tuple[tuple[str, tuple[object, ...], str], ...] = (
    ("ADDDATE", ("2026-11-27", 2), "ADDDATE('2026-11-27',2)"),
    ("ADDTIME", ("2026-11-27 12:00:00", "00:45:00"), "ADDTIME('2026-11-27 12:00:00','00:45:00')"),
    ("CONVERT_TZ", ("2026-11-27 12:00:00", "+02:00", "+00:00"),
     "CONVERT_TZ('2026-11-27 12:00:00','+02:00','+00:00')"),
    ("CURRENT_DATE", (), "CURRENT_DATE()"),
    ("CURRENT_TIME", (), "CURRENT_TIME()"),
    ("CURRENT_TIMESTAMP", (), "CURRENT_TIMESTAMP()"),
    ("DATE", ("2026-11-27 22:00:00",), "DATE('2026-11-27 22:00:00')"),
    ("DATE_ADD", (sqlfunc.DATE("2026-11-27"), sqlintv.DAY(2)), "DATE_ADD(DATE('2026-11-27'),INTERVAL 2 DAY)"),
    ("DATE_FORMAT", ("2026-11-27", "%Y/%m"), "DATE_FORMAT('2026-11-27','%Y/%m')"),
    ("DATE_SUB", (sqlfunc.DATE("2026-11-27"), sqlintv.DAY(2)), "DATE_SUB(DATE('2026-11-27'),INTERVAL 2 DAY)"),
    ("DATEDIFF", ("2026-11-27", "2026-11-20"), "DATEDIFF('2026-11-27','2026-11-20')"),
    ("DAY", ("2026-11-27",), "DAY('2026-11-27')"),
    ("DAYNAME", ("2026-11-27",), "DAYNAME('2026-11-27')"),
    ("DAYOFWEEK", ("2026-11-27",), "DAYOFWEEK('2026-11-27')"),
    ("DAYOFYEAR", ("2026-11-27",), "DAYOFYEAR('2026-11-27')"),
    ("EXTRACT", ("year_month", "2026-11-27"), "EXTRACT(YEAR_MONTH FROM '2026-11-27')"),
    ("FROM_DAYS", (739947,), "FROM_DAYS(739947)"),
    ("FROM_UNIXTIME", (0, "%Y-%m-%d"), "FROM_UNIXTIME(0,'%Y-%m-%d')"),
    ("GET_FORMAT", ("date", "ISO"), "GET_FORMAT(DATE,'ISO')"),
    ("HOUR", ("12:34:56",), "HOUR('12:34:56')"),
    ("LAST_DAY", ("2024-02-10",), "LAST_DAY('2024-02-10')"),
    ("MAKEDATE", (2024, 60), "MAKEDATE(2024,60)"),
    ("MAKETIME", (12, 34, 56), "MAKETIME(12,34,56)"),
    ("MICROSECOND", ("12:34:56.125",), "MICROSECOND('12:34:56.125')"),
    ("MINUTE", ("12:34:56",), "MINUTE('12:34:56')"),
    ("MONTH", ("2026-11-27",), "MONTH('2026-11-27')"),
    ("MONTHNAME", ("2026-11-27",), "MONTHNAME('2026-11-27')"),
    ("NOW", (), "NOW()"),
    ("PERIOD_ADD", (202611, 2), "PERIOD_ADD(202611,2)"),
    ("PERIOD_DIFF", (202701, 202611), "PERIOD_DIFF(202701,202611)"),
    ("QUARTER", ("2026-11-27",), "QUARTER('2026-11-27')"),
    ("SEC_TO_TIME", (90,), "SEC_TO_TIME(90)"),
    ("SECOND", ("12:34:56",), "SECOND('12:34:56')"),
    ("STR_TO_DATE", ("2026/11/27", "%Y/%m/%d"), "STR_TO_DATE('2026/11/27','%Y/%m/%d')"),
    ("SUBDATE", ("2026-11-27", 2), "SUBDATE('2026-11-27',2)"),
    ("SUBTIME", ("2026-11-27 12:00:00", "00:45:00"), "SUBTIME('2026-11-27 12:00:00','00:45:00')"),
    ("SYSDATE", (), "SYSDATE()"),
    ("TIME", ("2026-11-27 12:34:56",), "TIME('2026-11-27 12:34:56')"),
    ("TIME_FORMAT", ("12:34:56", "%H:%i"), "TIME_FORMAT('12:34:56','%H:%i')"),
    ("TIME_TO_SEC", ("00:01:30",), "TIME_TO_SEC('00:01:30')"),
    ("TIMEDIFF", ("12:34:56", "12:33:00"), "TIMEDIFF('12:34:56','12:33:00')"),
    ("TIMESTAMP", ("2026-11-27", "12:34:56"), "TIMESTAMP('2026-11-27','12:34:56')"),
    ("TIMESTAMPADD", ("SQL_TSI_DAY", 2, "2026-11-27"), "TIMESTAMPADD(SQL_TSI_DAY,2,'2026-11-27')"),
    ("TIMESTAMPDIFF", ("DAY", "2026-11-20", "2026-11-27"), "TIMESTAMPDIFF(DAY,'2026-11-20','2026-11-27')"),
    ("TO_DAYS", ("2026-11-27",), "TO_DAYS('2026-11-27')"),
    ("TO_SECONDS", ("2026-11-27 12:34:56",), "TO_SECONDS('2026-11-27 12:34:56')"),
    ("UNIX_TIMESTAMP", ("2026-11-27 12:34:56",), "UNIX_TIMESTAMP('2026-11-27 12:34:56')"),
    ("UTC_DATE", (), "UTC_DATE()"),
    ("UTC_TIME", (), "UTC_TIME()"),
    ("UTC_TIMESTAMP", (), "UTC_TIMESTAMP()"),
    ("WEEK", ("2026-11-27", 3), "WEEK('2026-11-27',3)"),
    ("WEEKDAY", ("2026-11-27",), "WEEKDAY('2026-11-27')"),
    ("WEEKOFYEAR", ("2026-11-27",), "WEEKOFYEAR('2026-11-27')"),
    ("YEAR", ("2026-11-27",), "YEAR('2026-11-27')"),
    ("YEARWEEK", ("2026-11-27", 3), "YEARWEEK('2026-11-27',3)"),
)


def test_temporal_names_roles_and_fractional_precision() -> None:
    assert len({name for name, _, _ in CASES}) == 55
    for name, args, _ in CASES:
        constructor: type[object] = getattr(sqlfunc, name)
        assert name in sqlfunc.__all__
        assert constructor.__module__ == "sqlaero.sqlfunc"
        assert constructor.__base__ is sqlfunc.ABS.__base__
        assert constructor.__doc__
        signature(constructor).bind(*args)

    for constructor in (sqlfunc.CURRENT_TIME, sqlfunc.CURRENT_TIMESTAMP, sqlfunc.NOW,
                        sqlfunc.SYSDATE, sqlfunc.UTC_TIME, sqlfunc.UTC_TIMESTAMP):
        for invalid in (-1, 7, 10**100):
            with pytest.raises(ValueError, match="fsp"):
                constructor(invalid)
        with pytest.raises(TypeError, match="fsp"):
            constructor(True)
        with pytest.raises(TypeError, match="fsp"):
            constructor(None)  # type: ignore[arg-type]

    for constructor in (sqlfunc.TIMESTAMPADD, sqlfunc.TIMESTAMPDIFF):
        with pytest.raises(ValueError, match="unit"):
            constructor("DAY_SECOND", 1, "2026-11-27")
    with pytest.raises(ValueError, match="type"):
        sqlfunc.GET_FORMAT("YEAR", "ISO")
    with pytest.raises(TypeError, match="interval"):
        sqlfunc.DATE_SUB("2026-11-27", 2)  # type: ignore[arg-type]
    seen: list[int] = []
    with pytest.raises(TypeError, match="unit"):
        sqlfunc.TIMESTAMPADD(CustomEscape(1, lambda v: seen.append(v) or "DAY"), 2, "2026-11-27")  # type: ignore[arg-type]
    assert not seen


@requires_mysql
@pytest.mark.parametrize(("name", "args", "native"), CASES)
def test_temporal_function_matches_native(
    connection: Connection, name: str, args: tuple[object, ...], native: str,
) -> None:
    constructor: Callable[..., EscapeInput] = getattr(sqlfunc, name)
    expr = constructor(*args)
    assert str(connection.escape(expr)).startswith(f"{name}(")
    with connection.cursor() as cursor:
        cursor.execute(f"SELECT %s,{native}", expr)
        row = cursor.fetchone()
        assert row is not None and row[0] is not None
        if name == "SYSDATE":
            assert isinstance(row[0], datetime) and isinstance(row[1], datetime)
            assert abs(row[0] - row[1]) <= timedelta(seconds=1)
        else:
            assert row[0] == row[1]


@requires_mysql
def test_clocks_and_date_interval_variants(connection: Connection) -> None:
    constructors = (sqlfunc.CURRENT_TIME, sqlfunc.CURRENT_TIMESTAMP, sqlfunc.NOW,
                    sqlfunc.SYSDATE, sqlfunc.UTC_TIME, sqlfunc.UTC_TIMESTAMP)
    with connection.cursor() as cursor:
        for constructor in constructors:
            for fsp, expected in ((None, "()"), (0, "(0)"), (3, "(3)"), (6, "(6)")):
                expr = constructor() if fsp is None else constructor(fsp)
                assert str(connection.escape(expr)) == constructor.__name__ + expected
                cursor.execute("SELECT %s", expr)
                row = cursor.fetchone()
                assert row is not None and isinstance(row[0], (datetime, timedelta))
                fraction = row[0].microsecond if isinstance(row[0], datetime) else row[0].microseconds
                assert fraction % 10 ** (6 - (fsp or 0)) == 0

        target = sqlfunc.DATE("2026-11-27")
        interval = sqlintv.DAY(sqlfunc.ABS(-2))
        cursor.execute("SELECT %s,%s,%s", (
            sqlfunc.ADDDATE(target, interval),
            sqlfunc.SUBDATE(target, interval),
            sqlfunc.DATE_SUB(target, sqlintv.YEAR_MONTH(1, 2)),
        ))
        assert cursor.fetchone() == (date(2026, 11, 29), date(2026, 11, 25), date(2025, 9, 27))
