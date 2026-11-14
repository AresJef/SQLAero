"""Arrow temporal and fixed Decimal facts preserve source precision and sign."""

# pyright: reportPrivateUsage=false
from typing import Any
from collections.abc import Callable


from decimal import Decimal
import json
import struct

import pytest
from sqlaero import _sqlaero, json_dumps
from sqlaero import Cursor, DictCursor, SSCursor, SSDictCursor
from sqlaero.errors import InterfaceError, SQLAeroErrorCode
from test_public_connection import live, requires_mysql

pa = pytest.importorskip("pyarrow")


@pytest.mark.parametrize("zone", ["UTC\x00bad", "\x00bad"])
@pytest.mark.parametrize("container", ["array", "chunked", "list", "struct", "dictionary", "table", "batch", "pandas"])
def test_timezone_metadata_is_not_truncated_at_c_data_nul(zone: str, container: str) -> None:
    value = pa.array([0], type=pa.timestamp("ns", zone))
    if container == "chunked":
        value = pa.chunked_array([value.slice(0, 0), value])
    elif container == "list":
        value = pa.ListArray.from_arrays(pa.array([0, 1], type=pa.int32()), value)
    elif container == "struct":
        value = pa.StructArray.from_arrays([value], names=["stamp"])
    elif container == "dictionary":
        value = pa.DictionaryArray.from_arrays(pa.array([0], type=pa.int8()), value)
    elif container == "table":
        value = pa.table({"stamp": value})
    elif container == "batch":
        value = pa.record_batch([value], names=["stamp"])
    elif container == "pandas":
        pd = pytest.importorskip("pandas")
        value = pd.arrays.ArrowExtensionArray(value)
    # The provider's full timezone is invalid. A C-string prefix may not turn
    # it into UTC or a naive timestamp. The real protocol error is preserved.
    with pytest.raises((ValueError, KeyError), match="null|NUL|embedded|time zone|Time zone|ZoneInfo"):
        json_dumps(value)


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("kind", [Cursor, DictCursor, SSCursor, SSDictCursor])
@pytest.mark.parametrize("prepared", [False, True])
def test_complete_timezone_fact_keeps_mysql_aware_session_guard(label: str, kind: type[Cursor] | type[DictCursor] | type[SSCursor] | type[SSDictCursor], prepared: bool) -> None:
    value = pa.array([0], type=pa.timestamp("ns", "\x00bad"))
    with live(label) as connection, connection.cursor(kind, prepared=prepared) as current:
        # MySQL consumes the UTC instant, not the source's local-zone name.
        current.execute("SELECT CAST(%s AS DATETIME) AS stamp", value)
        from datetime import datetime
        assert current.fetchone() == ({"stamp": datetime(1970, 1, 1)} if kind in (DictCursor, SSDictCursor) else (datetime(1970, 1, 1),))
        assert current.fetchone() is None
        current.execute("SET SESSION time_zone = '+02:00'")
        with pytest.raises(InterfaceError) as caught:
            current.execute("SELECT %s", value)
        assert caught.value.code == SQLAeroErrorCode.SESSION_STATE
        current.execute("SELECT 17 AS value")
        assert current.fetchone() == ({"value": 17} if kind in (DictCursor, SSDictCursor) else (17,))


@pytest.mark.parametrize(("dtype", "ticks", "expected"), [
    (pa.date32(), 0, "1970-01-01"),
    (pa.date32(), -1, "1969-12-31"),
    (pa.date64(), -86_400_000, "1969-12-31"),
    (pa.timestamp("s"), 0, "1970-01-01T00:00:00"),
    (pa.timestamp("ms"), -1, "1969-12-31T23:59:59.999000"),
    (pa.timestamp("us"), -1, "1969-12-31T23:59:59.999999"),
    (pa.timestamp("ns"), -1, "1969-12-31T23:59:59.999999999"),
    (pa.timestamp("ns", "UTC"), 1, "1970-01-01T00:00:00.000000001+00:00"),
    (pa.timestamp("ns", "+08:00"), -1, "1970-01-01T07:59:59.999999999+08:00"),
    (pa.timestamp("ns", "-03:30"), 1, "1969-12-31T20:30:00.000000001-03:30"),
    (pa.timestamp("s", "America/New_York"), 0, "1969-12-31T19:00:00-05:00"),
    (pa.timestamp("s", "America/New_York"), 1593561600, "2020-06-30T20:00:00-04:00"),
    (pa.duration("s"), -1, "-PT1S"),
    (pa.duration("ms"), 1001, "PT1.001S"),
    (pa.duration("us"), -1, "-PT0.000001S"),
    (pa.duration("ns"), -900, "-PT0.0000009S"),
    (pa.time32("s"), 90000, "25:00:00"),
    (pa.time32("s"), -3600, "-01:00:00"),
    (pa.time32("ms"), -1, "-00:00:00.001000"),
    (pa.time64("us"), 1, "00:00:00.000001"),
    (pa.time64("ns"), -900, "-00:00:00.000000900"),
])
@pytest.mark.parametrize("chunked", [False, True])
def test_actual_temporal_ticks_are_not_boxed_or_truncated(dtype: Any, ticks: int, expected: str, chunked: bool) -> None:
    # Public buffers preserve actual physical ticks, including wide/negative
    # clock values whose Python time constructor cannot represent them.
    width = dtype.bit_width
    data = pa.py_buffer(struct.pack("=i" if width == 32 else "=q", ticks))
    value = pa.Array.from_buffers(dtype, 1, [None, data])
    if chunked:
        value = pa.chunked_array([value.slice(0, 0), value])
    assert json.loads(json_dumps(value)) == [expected]


@pytest.mark.parametrize("factory", [pa.decimal32, pa.decimal64, pa.decimal128, pa.decimal256])
@pytest.mark.parametrize(("scale", "value", "expected"), [
    (3, "12.500", "12.500"),
    (3, "0.000", "0.000"),
    (-2, "12300", "1.23E+4"),
])
def test_native_decimal_scale_is_a_fact_not_sql_or_json_policy(factory: Callable[..., Any], scale: int, value: str, expected: str) -> None:
    array = pa.array([Decimal(value), None], type=factory(8, scale))
    assert json.loads(json_dumps(array)) == [expected, None]
    sql = _sqlaero._escape_projection(array)
    assert isinstance(sql, tuple)
    assert Decimal(sql[0]) == Decimal(value)
    assert sql[1] == "NULL"


def test_decimal256_exact_66_digit_integer_is_json_text_but_not_mysql_numeric() -> None:
    value = "1" + "0" * 65
    array = pa.array([Decimal(value)], type=pa.decimal256(76, 0))
    assert json_dumps(array) == ('["' + value + '"]').encode()
    with pytest.raises(_sqlaero.DataError, match="65"):
        _sqlaero._escape_projection(array)


def test_fixed_utc_wide_year_avoids_python_calendar_narrowing() -> None:
    np = pytest.importorskip("numpy")
    ticks = int(np.datetime64("12000-01-01", "s").astype("int64"))
    value = pa.array([ticks], type=pa.timestamp("s", "UTC"))
    assert json_dumps(value) == b'["12000-01-01T00:00:00+00:00"]'


def test_valid_arrow_minimum_tick_is_not_numpy_nat() -> None:
    value = pa.array([-(2**63)], type=pa.duration("ns"))
    assert json_dumps(value) != b"[null]"


def test_date64_non_integral_day_is_not_silently_rounded() -> None:
    value = pa.Array.from_buffers(pa.date64(), 1, [None, pa.py_buffer(struct.pack("=q", -1))])
    with pytest.raises(_sqlaero.DataError, match="integral day"):
        json_dumps(value)
