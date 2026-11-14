"""Pandas sources expose actual native facts to both immediate targets."""

from collections.abc import Callable
import json
from typing import Any
from datetime import timezone, timedelta
from zoneinfo import ZoneInfo

import pytest
from sqlaero import _sqlaero, json_dumps

pd = pytest.importorskip("pandas")
np = pytest.importorskip("numpy")


@pytest.mark.parametrize(("factory", "expected"), [
    (lambda: pd.Series([7, 8], index=["u", "u"], name="id"), [7, 8]),
    (lambda: pd.Index([7, 8], name="ignored"), [7, 8]),
    (lambda: pd.RangeIndex(5, 0, -2), [5, 3, 1]),
    (lambda: pd.CategoricalIndex(["ab", None, "ab"]), ["ab", None, "ab"]),
    (lambda: pd.Categorical(["ab", None, "cd"]), ["ab", None, "cd"]),
    (lambda: pd.MultiIndex.from_tuples([(7, "ab"), (8, "cd")]), [[7, "ab"], [8, "cd"]]),
    (lambda: pd.array([7, None, 8], dtype="Int64"), [7, None, 8]),
    (lambda: pd.array([0.5, None], dtype="Float64"), [0.5, None]),
    (lambda: pd.array([True, None, False], dtype="boolean"), [True, None, False]),
    (lambda: pd.array(["ab", None], dtype="string[python]"), ["ab", None]),
    (lambda: pd.arrays.NumpyExtensionArray(np.array([7, 8])), [7, 8]),
    (lambda: pd.array(["2026-11-12T08:09:10.123456789", None], dtype="datetime64[ns]"), ["2026-11-12T08:09:10.123456789", None]),
    (lambda: pd.array([900, None], dtype="timedelta64[ns]"), ["PT0.0000009S", None]),
    (lambda: pd.DatetimeIndex(["2026-11-12T08:09:10.123456789", None]), ["2026-11-12T08:09:10.123456789", None]),
    (lambda: pd.to_timedelta([900, None], unit="ns"), ["PT0.0000009S", None]),
    (lambda: pd.DataFrame({"id": [7, 8], "name": ["ab", "cd"]}), [{"id": 7, "name": "ab"}, {"id": 8, "name": "cd"}]),
])
def test_pandas_actual_values_reach_json_without_sql_or_row_materialization(
    factory: Callable[[], object], expected: object,
) -> None:
    assert json.loads(json_dumps(factory())) == expected


@pytest.mark.parametrize("kind", ["series", "frame", "index", "range", "integer", "string", "numpy", "datetime", "timedelta", "categorical"])
def test_real_pandas_subclasses_ignore_all_replacement_attribute_access(kind: str) -> None:
    factories: dict[str, Callable[[], Any]] = {
        "series": lambda: pd.Series([7, 8]),
        "frame": lambda: pd.DataFrame({"id": [7, 8]}),
        "index": lambda: pd.Index([7, 8]),
        "range": lambda: pd.RangeIndex(7, 9),
        "integer": lambda: pd.array([7, None, 8], dtype="Int64"),
        "string": lambda: pd.array(["ab", None], dtype="string[python]"),
        "numpy": lambda: pd.arrays.NumpyExtensionArray(np.array([7, 8])),
        "datetime": lambda: pd.array(["2026-11-12", None], dtype="datetime64[ns]"),
        "timedelta": lambda: pd.array([900, None], dtype="timedelta64[ns]"),
        "categorical": lambda: pd.Categorical(["ab", None]),
    }
    value = factories[kind]()
    base = type(value)
    child = type("ActualChild", (base,), {"__slots__": ()})
    value.__class__ = child
    calls: list[str] = []

    def forbidden(self: object, name: str) -> Any:
        calls.append(name)
        raise AssertionError("Pandas replacement access ran")

    child.__getattribute__ = forbidden
    if kind == "frame":
        expected: object = [{"id": 7}, {"id": 8}]
    elif kind in ("series", "index", "range", "numpy"):
        expected = [7, 8]
    elif kind == "integer":
        expected = [7, None, 8]
    elif kind in ("string", "categorical"):
        expected = ["ab", None]
    elif kind == "datetime":
        expected = ["2026-11-12T00:00:00", None]
    else:
        expected = ["PT0.0000009S", None]
    assert json.loads(json_dumps(value)) == expected
    assert calls == []


@pytest.mark.parametrize("columns", [["id", "id"], [1, "1"]])
def test_pandas_json_checks_actual_output_names_only(columns: list[object]) -> None:
    value = pd.DataFrame([[7, 8]], columns=columns)
    with pytest.raises(ValueError, match="collid"):
        json_dumps(value)
    assert _sqlaero._escape_projection(value) == [("7", "8")]
    assert json_dumps(value.iloc[:0]) == b"[]"


def test_pandas_table_names_keep_embedded_nul_and_zero_width_rows() -> None:
    assert json_dumps(pd.DataFrame([[7, 8]], columns=["a\0x", "a\0y"])) == b'[{"a\\u0000x":7,"a\\u0000y":8}]'
    assert json_dumps(pd.DataFrame(index=[0, 1])) == b"[{},{}]"
    assert json_dumps(pd.DataFrame([[7]], columns=[np.uint64(2**64 - 1)])) == b'[{"18446744073709551615":7}]'


@pytest.mark.parametrize(("value", "expected"), [
    (pd.Timestamp("2026-11-12T08:09:10.123456789"), b'"2026-11-12T08:09:10.123456789"'),
    (pd.Timestamp("2026-11-12T08:09:10.123456789+08:00"), b'"2026-11-12T08:09:10.123456789+08:00"'),
    (pd.Timestamp(np.datetime64("12000-01-01", "s")), b'"12000-01-01T00:00:00"'),
    (pd.Timedelta(-900, unit="ns"), b'"-PT0.0000009S"'),
])
def test_pandas_temporal_scalars_preserve_source_resolution_and_offset(value: object, expected: bytes) -> None:
    assert json_dumps(value) == expected


@pytest.mark.parametrize("kind", ["array", "index", "series", "frame"])
@pytest.mark.parametrize(("zone", "expected"), [
    (timezone.utc, ["2026-11-01T05:30:00.123456789+00:00", "2026-11-01T06:30:00.123456789+00:00"]),
    (timezone(timedelta(hours=8)), ["2026-11-01T13:30:00.123456789+08:00", "2026-11-01T14:30:00.123456789+08:00"]),
    (ZoneInfo("America/New_York"), ["2026-11-01T01:30:00.123456789-04:00", "2026-11-01T01:30:00.123456789-05:00"]),
])
def test_aware_pandas_arrays_preserve_each_actual_offset_and_full_precision(
    kind: str, zone: object, expected: list[str],
) -> None:
    value = pd.DatetimeIndex(["2026-11-01T05:30:00.123456789Z", "2026-11-01T06:30:00.123456789Z"]).tz_convert(zone)
    data = {"array": lambda: value.array, "index": lambda: value,
            "series": lambda: pd.Series(value), "frame": lambda: pd.DataFrame({"at": value})}[kind]()
    result: object = [{"at": item} for item in expected] if kind == "frame" else expected
    assert json.loads(json_dumps(data)) == result


def test_wide_aware_array_uses_native_seconds_without_a_python_datetime_narrowing() -> None:
    value = pd.DatetimeIndex(np.array(["12000-01-01"], dtype="M8[s]")).tz_localize(timezone.utc)
    assert json_dumps(value) == b'["12000-01-01T00:00:00+00:00"]'


def test_nullable_mask_is_distinct_from_an_actual_unmasked_nan() -> None:
    value = pd.arrays.FloatingArray(np.array([float("nan"), 1.5]), np.array([False, True]))
    assert json_dumps(value) == b"[null,null]"
    with pytest.raises(ValueError, match="nonfinite"):
        json_dumps(value, allow_nan=False)
    value = pd.arrays.FloatingArray(np.array([float("nan")]), np.array([True]))
    assert json_dumps(value, allow_nan=False) == b"[null]"


def test_string_missing_nan_is_not_a_valid_nonfinite_float_value() -> None:
    value = pd.array(["ab", None], dtype=pd.StringDtype(storage="python", na_value=np.nan))
    assert json_dumps(value, allow_nan=False) == b'["ab",null]'
