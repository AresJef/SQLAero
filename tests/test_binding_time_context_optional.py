"""Statement UTC facts reach native provider columns and object-cell callbacks."""

from collections.abc import Generator
from contextlib import contextmanager
from datetime import datetime, timedelta, timezone, tzinfo
from typing import Any
from uuid import uuid4

import pytest
from sqlaero import Connection
from sqlaero.errors import InterfaceError, SQLAeroErrorCode
from sqlaero.typing import EscapeInput
from test_public_connection import live, requires_mysql


AWARE = datetime(2024, 1, 2, 3, 4, 5, 123456, tzinfo=timezone(timedelta(hours=8)))
NAIVE = AWARE.replace(tzinfo=None)
UTC_VALUE = AWARE.astimezone(timezone.utc).replace(tzinfo=None)
KINDS = ("numpy_object", "pandas_series", "pandas_index", "pandas_arrow",
         "arrow_array", "arrow_batch", "arrow_table", "polars_series", "polars_table")


def provider_input(kind: str, values: list[datetime | None], aware: bool = True) -> EscapeInput:
    """Construct public provider objects without importing missing families at collection."""
    zone = "Asia/Shanghai" if aware else None
    if kind == "numpy_object":
        np: Any = pytest.importorskip("numpy")
        return np.array([[value] for value in values], dtype=object)
    if kind.startswith("pandas_"):
        pd: Any = pytest.importorskip("pandas")
        if kind == "pandas_arrow":
            pa: Any = pytest.importorskip("pyarrow")
            return pd.Series(values, dtype=pd.ArrowDtype(pa.timestamp("us", tz=zone)))
        dtype = pd.DatetimeTZDtype(unit="us", tz=zone) if aware else "datetime64[us]"
        if kind == "pandas_index":
            return pd.DatetimeIndex(values, dtype=dtype)
        return pd.Series(values, dtype=dtype)
    if kind.startswith("arrow_"):
        pa = pytest.importorskip("pyarrow")
        array = pa.array(values, type=pa.timestamp("us", tz=zone))
        if kind == "arrow_array":
            return array
        if kind == "arrow_batch":
            return pa.RecordBatch.from_arrays([array], names=["stamp"])
        return pa.table({"stamp": array})
    pl: Any = pytest.importorskip("polars")
    series = pl.Series("stamp", values, dtype=pl.Datetime("us", zone))
    return pl.DataFrame({"stamp": series}) if kind == "polars_table" else series


@contextmanager
def timestamp_table(connection: Connection) -> Generator[str, None, None]:
    """Keep each test's target private to its own connection."""
    name = "a_optional_utc_" + uuid4().hex
    with connection.cursor() as setup:
        setup.execute(f"CREATE TEMPORARY TABLE {name}(stamp TIMESTAMP(6))")
        try:
            yield name
        finally:
            setup.execute(f"DROP TEMPORARY TABLE IF EXISTS {name}")


def target_counts(connection: Connection) -> dict[str, int]:
    """Read only fixed, nonsensitive counters for target sends and execution."""
    result: dict[str, int] = {}
    with connection.cursor() as observer:
        observer.execute("SHOW SESSION STATUS WHERE Variable_name IN "
                         "('Com_insert','Com_stmt_prepare','Com_stmt_execute')")
        for name, value in observer.fetchall():
            assert isinstance(name, str) and isinstance(value, str) and value.isdecimal()
            result[name] = int(value)
    assert set(result) == {"Com_insert", "Com_stmt_prepare", "Com_stmt_execute"}
    return result


def assert_local_context_failure(error: InterfaceError) -> None:
    """A local UTC refusal identifies the hint without becoming a server error."""
    assert error.code == SQLAeroErrorCode.SESSION_STATE and error.is_server_error is False
    notes = "\n".join(getattr(error, "__notes__", ()))
    assert "SET_VAR" in notes and "time_zone" in notes and "SQL character" in notes


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("prepared", [False, True])
@pytest.mark.parametrize("kind", KINDS)
def test_optional_aware_cells_reject_non_utc_hint_before_target_send(
    label: str, prepared: bool, kind: str,
) -> None:
    source = provider_input(kind, [AWARE])
    with live(label) as connection, timestamp_table(connection) as table:
        before = target_counts(connection)
        with connection.cursor(prepared=prepared) as cursor:
            with pytest.raises(InterfaceError) as failure:
                cursor.execute(f"INSERT /*+ SET_VAR(time_zone='+02:00') */ INTO {table} VALUES (%s)",
                               source, many=True)
            assert_local_context_failure(failure.value)
        assert target_counts(connection) == before
        with connection.cursor() as observer:
            observer.execute(f"SELECT COUNT(*) FROM {table}")
            assert observer.fetchone() == (0,)


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("prepared", [False, True])
@pytest.mark.parametrize("kind", ["numpy_object", "arrow_table"])
def test_optional_aware_cells_reject_an_unconfirmed_system_timezone_hint(
    label: str, prepared: bool, kind: str,
) -> None:
    source = provider_input(kind, [AWARE])
    with live(label) as connection, timestamp_table(connection) as table:
        before = target_counts(connection)
        with connection.cursor(prepared=prepared) as cursor:
            with pytest.raises(InterfaceError) as failure:
                cursor.execute(f"INSERT /*+ SET_VAR(time_zone='SYSTEM') */ INTO {table} VALUES (%s)",
                               source, many=True)
            assert_local_context_failure(failure.value)
        assert target_counts(connection) == before


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("prepared", [False, True])
@pytest.mark.parametrize("kind", KINDS)
def test_optional_all_null_timezone_columns_still_write_sql_null(
    label: str, prepared: bool, kind: str,
) -> None:
    source = provider_input(kind, [None, None])
    with live(label) as connection, timestamp_table(connection) as table:
        with connection.cursor(prepared=prepared) as cursor:
            assert cursor.execute(f"INSERT /*+ SET_VAR(time_zone='+02:00') */ INTO {table} VALUES (%s)",
                                  source, many=True) == 2
        with connection.cursor() as observer:
            observer.execute(f"SELECT stamp,@@time_zone FROM {table}")
            assert observer.fetchall() == ((None, "+00:00"), (None, "+00:00"))


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("prepared", [False, True])
@pytest.mark.parametrize("kind", ["numpy_object", "pandas_series", "pandas_arrow", "arrow_table", "polars_table"])
@pytest.mark.parametrize("scenario", ["naive_non_utc", "aware_non_timezone"])
def test_optional_naive_or_unrelated_hint_does_not_create_a_timezone_refusal(
    label: str, prepared: bool, kind: str, scenario: str,
) -> None:
    aware = scenario == "aware_non_timezone"
    source = provider_input(kind, [AWARE if aware else NAIVE, None], aware=aware)
    hint = "sort_buffer_size=32768" if aware else "time_zone='+02:00'"
    expected = UTC_VALUE if aware else NAIVE - timedelta(hours=2)
    with live(label) as connection, timestamp_table(connection) as table:
        with connection.cursor(prepared=prepared) as cursor:
            assert cursor.execute(f"INSERT /*+ SET_VAR({hint}) */ INTO {table} VALUES (%s)",
                                  source, many=True) == 2
        with connection.cursor() as observer:
            observer.execute(f"SELECT stamp,@@time_zone FROM {table} ORDER BY stamp")
            assert observer.fetchall() == ((None, "+00:00"), (expected, "+00:00"))


class OffsetProtocol(tzinfo):
    """Use the existing tzinfo offset protocol inside an exact object array."""
    def __init__(self, none_offset: bool) -> None:
        self.none_offset = none_offset
        self.calls = 0

    def utcoffset(self, dt: datetime | None) -> timedelta | None:
        self.calls += 1
        return None if self.none_offset else timedelta(hours=8)

    def dst(self, dt: datetime | None) -> timedelta | None:
        return None

    def tzname(self, dt: datetime | None) -> str:
        return "optional-provider-offset"


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("prepared", [False, True])
@pytest.mark.parametrize("none_offset", [False, True])
def test_numpy_object_timezone_callback_uses_its_real_aware_or_naive_result(
    label: str, prepared: bool, none_offset: bool,
) -> None:
    np: Any = pytest.importorskip("numpy")
    zone = OffsetProtocol(none_offset)
    value = NAIVE.replace(tzinfo=zone)
    source = np.array([[value]], dtype=object)
    zone.calls = 0
    with live(label) as connection, timestamp_table(connection) as table:
        before = target_counts(connection)
        with connection.cursor(prepared=prepared) as cursor:
            sql = f"INSERT /*+ SET_VAR(time_zone='+02:00') */ INTO {table} VALUES (%s)"
            if none_offset:
                assert cursor.execute(sql, source, many=True) == 1
            else:
                with pytest.raises(InterfaceError) as failure:
                    cursor.execute(sql, source, many=True)
                assert_local_context_failure(failure.value)
        assert zone.calls > 0
        if not none_offset:
            assert target_counts(connection) == before
        with connection.cursor() as observer:
            observer.execute(f"SELECT stamp FROM {table}")
            assert observer.fetchall() == (((NAIVE - timedelta(hours=2),),) if none_offset else ())
