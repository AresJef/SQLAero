"""Arrow temporal and fixed Decimal facts preserve source precision and sign."""

from decimal import Decimal
import json
import struct

import pytest
from sqlaero import _sqlaero, json_dumps

pa = pytest.importorskip("pyarrow")


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
def test_actual_temporal_ticks_are_not_boxed_or_truncated(dtype, ticks: int, expected: str, chunked: bool) -> None:
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
def test_native_decimal_scale_is_a_fact_not_sql_or_json_policy(factory, scale: int, value: str, expected: str) -> None:
    array = pa.array([Decimal(value), None], type=factory(8, scale))
    assert json.loads(json_dumps(array)) == [expected, None]
    sql = _sqlaero._escape_projection(array)
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
