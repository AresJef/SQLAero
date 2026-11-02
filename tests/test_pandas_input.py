"""Pandas values use public storage and retain the native SQL value contract."""

# pyright: reportPrivateUsage=false

import pytest
import subprocess
import sys
from typing import Any

from sqlaero import _sqlaero
from sqlaero._sqlaero import DataError, ProgrammingError

pd = pytest.importorskip("pandas")
np = pytest.importorskip("numpy")


@pytest.mark.parametrize("value", [pd.NA, pd.NaT])
def test_pandas_missing_scalars_are_null_before_other_protocols(value: object) -> None:
    assert _sqlaero._encode_value(value).to_bytes() == b"NULL"


@pytest.mark.parametrize(("value", "expected"), [
    (pd.Timestamp("2024-01-02 03:04:05.123456789"), b"'2024-01-02 03:04:05.123456789'"),
    (pd.Timestamp("1969-12-31 23:59:59.999999999"), b"'1969-12-31 23:59:59.999999999'"),
    (pd.Timestamp("2500-01-01 00:00:00"), b"'2500-01-01 00:00:00'"),
    (pd.Timestamp("2024-01-02 03:04:05.123456789", tz="Asia/Shanghai"), b"'2024-01-01 19:04:05.123456789'"),
    (pd.Timedelta(1, unit="ns"), b"'00:00:00.000000001'"),
    (pd.Timedelta(-1, unit="ns"), b"'-00:00:00.000000001'"),
    (pd.Timedelta(2, unit="D"), b"'48:00:00'"),
])
def test_pandas_temporal_scalar_retains_source_unit_fraction_and_utc(value: object, expected: bytes) -> None:
    assert _sqlaero._encode_value(value).to_bytes() == expected


@pytest.mark.parametrize("unit", ["s", "ms", "us", "ns"])
def test_pandas_temporal_public_asm8_resolution_is_not_forced_to_ns(unit: str) -> None:
    timestamp = pd.Timestamp("2024-01-02 03:04:05").as_unit(unit)
    duration = pd.Timedelta(1, unit="s").as_unit(unit)
    assert _sqlaero._encode_value(timestamp).to_bytes() == b"'2024-01-02 03:04:05'"
    assert _sqlaero._encode_value(duration).to_bytes() == b"'00:00:01'"


def test_pandas_missing_class_name_does_not_register_arbitrary_objects() -> None:
    fake = type("NAType", (), {})()
    with pytest.raises(TypeError):
        _sqlaero._encode_value(fake)


@pytest.mark.parametrize("constructor", [pd.Series, pd.Index])
@pytest.mark.parametrize(("many", "itemize", "sql", "expected"), [
    (False, True, "SELECT %s,%s", [b"\x03SELECT 1,2"]),
    (False, False, "SELECT %s", [b"\x03SELECT (1,2)"]),
    (True, True, "SELECT %s", [b"\x03SELECT 1", b"\x03SELECT 2"]),
    (True, False, "SELECT %s", [b"\x03SELECT 1", b"\x03SELECT 2"]),
])
def test_pandas_vector_roles_match_logical_typed_cells(constructor: type, many: bool, itemize: bool, sql: str, expected: list[bytes]) -> None:
    value = constructor([1, 2], dtype=np.int64)
    assert _sqlaero._bind_sql(sql, value, many=many, itemize=itemize) == expected


@pytest.mark.parametrize("dtype", ["Int8", "Int16", "Int32", "Int64", "UInt8", "UInt16", "UInt32", "UInt64"])
def test_pandas_standalone_nullable_integer_array_uses_typed_data_and_mask(dtype: str) -> None:
    value = pd.array([1, None, 2] * 11, dtype=dtype)
    assert _sqlaero._bind_sql("SELECT %s", value, many=True) == [b"\x03SELECT 1", b"\x03SELECT NULL", b"\x03SELECT 2"] * 11


def test_pandas_nullable_uint64_preserves_the_unsigned_maximum() -> None:
    value = pd.array([2**64 - 1, None, 0] * 11, dtype="UInt64")
    assert _sqlaero._bind_sql("SELECT %s", value, many=True) == [b"\x03SELECT 18446744073709551615", b"\x03SELECT NULL", b"\x03SELECT 0"] * 11


@pytest.mark.parametrize("dtype", ["Float32", "Float64"])
def test_pandas_nullable_float_mask_and_binary_value_are_distinct(dtype: str) -> None:
    value = pd.array([0.1, None, -0.0] * 11, dtype=dtype)
    rows = _sqlaero._bind_sql("SELECT %s", value, many=True)
    assert float(rows[0].removeprefix(b"\x03SELECT ")).hex() == float(value[0]).hex()
    assert rows[1] == b"\x03SELECT NULL"
    assert float(rows[2].removeprefix(b"\x03SELECT ")).hex() == "-0x0.0p+0"


def test_pandas_standalone_boolean_array_uses_its_null_mask() -> None:
    value = pd.array([True, None, False] * 11, dtype="boolean")
    assert _sqlaero._bind_sql("SELECT %s", value, many=True) == [b"\x03SELECT 1", b"\x03SELECT NULL", b"\x03SELECT 0"] * 11


@pytest.mark.parametrize("storage", ["python", "pyarrow"])
def test_pandas_standalone_string_storage_preserves_text_and_missing(storage: str) -> None:
    if storage == "pyarrow":
        pytest.importorskip("pyarrow")
    value = pd.array(["a", None, "中文'"], dtype=pd.StringDtype(storage=storage))
    assert _sqlaero._bind_sql("SELECT %s", value, many=True) == [
        b"\x03SELECT 'a'", b"\x03SELECT NULL", "\x03SELECT '中文\\''".encode(),
    ]


@pytest.mark.parametrize("na_value", [pd.NA, np.nan], ids=["pandas-na", "numpy-nan"])
@pytest.mark.parametrize("reverse", [False, True])
def test_pandas_python_string_null_normalization_keeps_both_missing_conventions_across_blocks(na_value: Any, reverse: bool) -> None:
    values = ["a", None, pd.NA, np.nan, "中'", ""]
    # Literal SQL follows the original logical inputs, independently of the
    # provider's normalized payload and the driver's choice of mask reader.
    literals = [b"'a'", b"NULL", b"NULL", b"NULL", "'中\\''".encode(), b"''"]
    value = pd.array(
        [values[index % len(values)] for index in range(1025)],
        dtype=pd.StringDtype(storage="python", na_value=na_value),
    )
    expected = [literals[index % len(literals)] for index in range(1025)]
    if reverse:
        value = value[::-1]
        expected.reverse()
    assert _sqlaero._bind_sql("SELECT %s", value, many=True) == [b"\x03SELECT " + item for item in expected]
    assert _sqlaero._encode_value(value).to_bytes() == b"(" + b",".join(expected) + b")"


@pytest.mark.parametrize("error_type", [MemoryError, KeyboardInterrupt])
def test_pandas_python_string_late_numpy_failure_keeps_the_original_and_row(monkeypatch: pytest.MonkeyPatch, error_type: type[BaseException]) -> None:
    value = pd.array(["ok"] * 1025, dtype=pd.StringDtype(storage="python"))
    original = error_type("original Python StringArray block conversion")
    to_numpy = pd.arrays.StringArray.to_numpy
    lengths: list[int] = []

    def convert(array: Any, *args: Any, **kwargs: Any) -> Any:
        lengths.append(len(array))
        assert kwargs["dtype"] is object
        assert kwargs["copy"] is False
        if len(lengths) == 2:
            raise original
        return to_numpy(array, *args, **kwargs)

    monkeypatch.setattr(pd.arrays.StringArray, "to_numpy", convert)
    with pytest.raises(error_type) as caught:
        _sqlaero._bind_sql("SELECT %s", value, many=True)
    assert caught.value is original
    assert lengths == [1024, 1]
    assert "args[1024]" in "\n".join(getattr(caught.value, "__notes__", []))


def test_pandas_standalone_arrow_extension_array_reuses_native_arrow() -> None:
    pa = pytest.importorskip("pyarrow")
    value = pd.array([1, None, 2], dtype=pd.ArrowDtype(pa.int64()))
    assert _sqlaero._bind_sql("SELECT %s", value, many=True) == [b"\x03SELECT 1", b"\x03SELECT NULL", b"\x03SELECT 2"]


def test_pandas_arrow_nested_array_keeps_each_list_as_one_cell() -> None:
    pa = pytest.importorskip("pyarrow")
    value = pd.array([[1, None], None, []], dtype=pd.ArrowDtype(pa.list_(pa.int64())))
    assert _sqlaero._bind_sql("SELECT %s", value, many=True) == [b"\x03SELECT (1,NULL)", b"\x03SELECT NULL", b"\x03SELECT ()"]


def test_pandas_categorical_only_encodes_referenced_labels() -> None:
    class Unused:
        def __repr__(self) -> str:
            raise AssertionError("unused category must not be inspected")
    labels = pd.Index(["used", Unused()], dtype=object)
    value = pd.Categorical.from_codes([0, -1, 0], categories=labels)
    assert _sqlaero._bind_sql("SELECT %s", value, many=True) == [b"\x03SELECT 'used'", b"\x03SELECT NULL", b"\x03SELECT 'used'"]


def test_pandas_range_index_uses_values_and_ignores_its_name() -> None:
    value = pd.RangeIndex(9, -2, -3, name=object())
    assert _sqlaero._bind_sql("SELECT %s", value, many=True) == [b"\x03SELECT 9", b"\x03SELECT 6", b"\x03SELECT 3", b"\x03SELECT 0"]


def test_pandas_multi_index_tuple_is_one_logical_cell() -> None:
    value = pd.MultiIndex.from_tuples([(1, "a"), (2, "b")], names=[object(), object()])
    assert _sqlaero._bind_sql("SELECT %s", value, many=True) == [b"\x03SELECT (1,'a')", b"\x03SELECT (2,'b')"]


@pytest.mark.parametrize("reverse", [False, True])
def test_pandas_multi_index_codes_keep_nulls_slices_and_one_tuple_cell(reverse: bool) -> None:
    first = [index % 3 - 1 for index in range(35)]
    second = [(index + 1) % 3 - 1 for index in range(35)]
    value = pd.MultiIndex(
        levels=[pd.Index([1, 2], dtype=np.int64), pd.Index(["a", "b"], dtype=object)],
        codes=[first, second],
    )[1:34]
    pairs = list(zip(first[1:34], second[1:34]))
    if reverse:
        value = value[::-1]
        pairs.reverse()
    rows = [
        b"(" + ([b"1", b"2"][a] if a >= 0 else b"NULL") + b"," +
        ([b"'a'", b"'b'"][b] if b >= 0 else b"NULL") + b")"
        for a, b in pairs
    ]
    assert _sqlaero._bind_sql("SELECT %s", value, many=True) == [b"\x03SELECT " + row for row in rows]
    assert _sqlaero._encode_value(value).to_bytes() == b"(" + b",".join(rows) + b")"


@pytest.mark.parametrize("code", [-2, 3])
def test_pandas_multi_index_unusual_code_keeps_original_late_getter_behavior(code: int) -> None:
    value = pd.MultiIndex(
        levels=[pd.Index([10, 20, 30]), pd.Index(["a"], dtype=object)],
        codes=[[0] * 39 + [code], [0] * 40], verify_integrity=False,
    )
    if code == -2:
        assert _sqlaero._bind_sql("SELECT %s", value, many=True) == (
            [b"\x03SELECT (10,'a')"] * 39 + [b"\x03SELECT (20,'a')"]
        )
    else:
        with pytest.raises(IndexError) as caught:
            _sqlaero._bind_sql("SELECT %s", value, many=True)
        assert "args[39]" in "\n".join(getattr(caught.value, "__notes__", ()))


@pytest.mark.parametrize("fail", [False, True])
def test_pandas_multi_index_real_subclass_values_hook_stays_once(fail: bool) -> None:
    calls: list[int] = []
    original = RuntimeError("actual MultiIndex values hook")
    class Index(pd.MultiIndex):
        @property
        def values(self) -> Any:
            calls.append(1)
            if fail:
                raise original
            result = np.empty(33, dtype=object)
            for index in range(33):
                result[index] = (7, "hook")
            return result
    value = Index(levels=[[1], ["ignored"]], codes=[[0] * 33, [0] * 33])
    assert type(value) is Index
    if fail:
        with pytest.raises(RuntimeError) as caught:
            _sqlaero._encode_value(value)
        assert caught.value is original
    else:
        assert _sqlaero._bind_sql("SELECT %s", value, many=True) == [b"\x03SELECT (7,'hook')"] * 33
    assert calls == [1]


def test_pandas_multi_index_gets_all_labels_before_converting_them(monkeypatch: pytest.MonkeyPatch) -> None:
    events: list[str] = []
    class Label(str):
        def __str__(self) -> str:
            text = str.__str__(self)
            events.append("convert:" + text)
            return text
    value = pd.MultiIndex(
        levels=[pd.Index([Label("a")], dtype=object), pd.Index([Label("b")], dtype=object)],
        codes=[[0] * 33, [0] * 33],
    )
    indices = {id(level): index for index, level in enumerate(value.levels)}
    getter = pd.Index.__getitem__
    def observed(index: Any, key: Any) -> Any:
        if id(index) in indices:
            events.append("get:" + str(indices[id(index)]))
        return getter(index, key)
    monkeypatch.setattr(pd.Index, "__getitem__", observed)
    assert _sqlaero._bind_sql("SELECT %s", value, many=True) == [b"\x03SELECT ('a','b')"] * 33
    assert events == ["get:0", "get:1", "convert:a", "convert:b"] * 33


@pytest.mark.parametrize(("dtype", "numbers"), [
    ("int8", [-128, 127]),
    ("int16", [-32768, 32767]),
    ("int32", [-2147483648, 2147483647]),
    ("int64", [-9223372036854775808, 9223372036854775807]),
])
@pytest.mark.parametrize("reverse", [False, True])
def test_pandas_multi_index_signed_widths_keep_random_codes_and_object_labels(dtype: str, numbers: list[int], reverse: bool) -> None:
    first = [1, 0, 1, 1, 0, 0] * 6
    second = [0, 1, 1, 0, 1, 0] * 6
    value = pd.MultiIndex(
        levels=[pd.Index(np.array(numbers, dtype=dtype)), pd.Index(["a", "b"], dtype=object)],
        codes=[first, second],
    )[1:35]
    pairs = list(zip(first[1:35], second[1:35]))
    if reverse:
        value = value[::-1]
        pairs.reverse()
    expected = [
        b"\x03SELECT (" + str(numbers[a]).encode() + b"," + [b"'a'", b"'b'"][b] + b")"
        for a, b in pairs
    ]
    assert _sqlaero._bind_sql("SELECT %s", value, many=True) == expected


@pytest.mark.parametrize(("many", "itemize"), [(False, False), (False, True), (True, False), (True, True)])
def test_pandas_multi_index_signed_and_object_values_keep_all_binding_roles(many: bool, itemize: bool) -> None:
    value = pd.MultiIndex(
        levels=[pd.Index([-7, 9], dtype=np.int64), pd.Index(["a", "b"], dtype=object)],
        codes=[[1, 0] * 17, [0, 1] * 17],
    )
    cells = [b"(9,'a')", b"(-7,'b')"] * 17
    if many:
        sql = "SELECT %s"
        expected = [b"\x03SELECT " + cell for cell in cells]
    elif itemize:
        sql = "SELECT " + ",".join(["%s"] * len(cells))
        expected = [b"\x03SELECT " + b",".join(cells)]
    else:
        sql = "SELECT %s"
        expected = [b"\x03SELECT (" + b",".join(cells) + b")"]
    assert _sqlaero._bind_sql(sql, value, many=many, itemize=itemize) == expected


def test_pandas_multi_index_signed_leaf_keeps_unsigned_and_float_values_exact() -> None:
    value = pd.MultiIndex(
        levels=[
            pd.Index([-9223372036854775808, 9223372036854775807], dtype=np.int64),
            pd.Index(np.array([0, 18446744073709551615], dtype=np.uint64)),
            pd.Index(np.array([-0.0, 0.1], dtype=np.float64)),
        ],
        codes=[[0, 1] * 17, [1, 0] * 17, [0, 1] * 17],
    )
    rows = _sqlaero._bind_sql("SELECT %s", value, many=True)
    expected_signed = [b"-9223372036854775808", b"9223372036854775807"] * 17
    expected_unsigned = [b"18446744073709551615", b"0"] * 17
    expected_float = ["-0x0.0p+0", "0x1.999999999999ap-4"] * 17
    for row, signed, unsigned, floating in zip(rows, expected_signed, expected_unsigned, expected_float):
        first, second, third = row.removeprefix(b"\x03SELECT (").removesuffix(b")").split(b",")
        assert (first, second) == (signed, unsigned)
        assert float(third).hex() == floating


def test_pandas_multi_index_signed_and_object_gets_every_label_before_conversion(monkeypatch: pytest.MonkeyPatch) -> None:
    events: list[str] = []
    class Label(str):
        def __str__(self) -> str:
            text = str.__str__(self)
            events.append("convert:" + text)
            return text
    value = pd.MultiIndex(
        levels=[pd.Index([7], dtype=np.int64), pd.Index([Label("a")], dtype=object), pd.Index([Label("b")], dtype=object)],
        codes=[[0] * 35, [0] * 35, [0] * 35],
    )
    targets = {id(value.levels[1]): "a", id(value.levels[2]): "b"}
    getter = pd.Index.__getitem__
    def observed(index: Any, key: Any) -> Any:
        if id(index) in targets:
            events.append("get:" + targets[id(index)])
        return getter(index, key)
    monkeypatch.setattr(pd.Index, "__getitem__", observed)
    assert _sqlaero._bind_sql("SELECT %s", value, many=True) == [b"\x03SELECT (7,'a','b')"] * 35
    assert events == ["get:a", "get:b", "convert:a", "convert:b"] * 35


@pytest.mark.parametrize("error_type", [MemoryError, KeyboardInterrupt])
def test_pandas_multi_index_later_object_getter_error_precedes_tuple_output_limit(monkeypatch: pytest.MonkeyPatch, error_type: type[BaseException]) -> None:
    value = pd.MultiIndex(
        levels=[pd.Index([7], dtype=np.int64), pd.Index(["a"], dtype=object)],
        codes=[[0] * 35, [0] * 35],
    )
    target = value.levels[1]
    original = error_type("original later MultiIndex label getter")
    getter = pd.Index.__getitem__
    def observed(index: Any, key: Any) -> Any:
        if index is target:
            raise original
        return getter(index, key)
    monkeypatch.setattr(pd.Index, "__getitem__", observed)
    # The SQL prefix fits, but even '(' would exceed this limit. The original
    # later label getter still wins: all labels precede any tuple output.
    with pytest.raises(error_type) as caught:
        _sqlaero._bind_sql("SELECT %s", value, many=True, limit=len(b"\x03SELECT "))
    assert caught.value is original


def test_pandas_multi_index_signed_leaf_is_owned_before_later_object_getter(monkeypatch: pytest.MonkeyPatch) -> None:
    value = pd.MultiIndex(
        levels=[pd.Index([7], dtype=np.int64), pd.Index(["a"], dtype=object)],
        codes=[[0] + [-1] * 34, [0] * 35],
    )
    target = value.levels[1]
    numbers = value.levels[0].to_numpy(copy=False)
    numbers.setflags(write=True)
    getter = pd.Index.__getitem__
    changed = False
    def observed(index: Any, key: Any) -> Any:
        nonlocal changed
        if index is target and not changed:
            changed = True
            numbers[0] = 99
        return getter(index, key)
    monkeypatch.setattr(pd.Index, "__getitem__", observed)
    rows = _sqlaero._bind_sql("SELECT %s", value, many=True)
    assert rows == [b"\x03SELECT (7,'a')"] + [b"\x03SELECT (NULL,'a')"] * 34
    assert changed


@pytest.mark.parametrize("code", [-1, -2, 3])
def test_pandas_multi_index_whole_row_fallback_does_not_prepare_signed_labels(monkeypatch: pytest.MonkeyPatch, code: int) -> None:
    value = pd.MultiIndex(
        levels=[pd.Index([10, 20, 30], dtype=np.int64), pd.Index(["a"], dtype=object)],
        codes=[[code] + [-1] * 34, [0] * 35], verify_integrity=False,
    )
    def unexpected(*args: Any, **kwargs: Any) -> Any:
        raise AssertionError("invalid/missing row must not prepare a signed label reader")
    monkeypatch.setattr(pd.arrays.NumpyExtensionArray, "to_numpy", unexpected)
    if code == 3:
        with pytest.raises(IndexError):
            _sqlaero._bind_sql("SELECT %s", value, many=True)
    else:
        first = b"NULL" if code == -1 else b"20"
        assert _sqlaero._bind_sql("SELECT %s", value, many=True) == (
            [b"\x03SELECT (" + first + b",'a')"] + [b"\x03SELECT (NULL,'a')"] * 34
        )


def test_pandas_multi_index_native_signed_limit_error_keeps_the_child_position() -> None:
    value = pd.MultiIndex(
        levels=[pd.Index([-9223372036854775808], dtype=np.int64), pd.Index(["a"], dtype=object)],
        codes=[[0] * 35, [0] * 35],
    )
    with pytest.raises(_sqlaero.OperationalError) as caught:
        _sqlaero._bind_sql("SELECT %s", value, many=True, limit=len(b"\x03SELECT (123"))
    assert "args[0][0]" in "\n".join(getattr(caught.value, "__notes__", []))


@pytest.mark.parametrize("wrappers", [62, 63])
def test_pandas_multi_index_keeps_the_original_tuple_depth(wrappers: int) -> None:
    value: object = pd.MultiIndex(
        levels=[pd.Index([1]), pd.Index(["a"], dtype=object)], codes=[[0] * 33, [0] * 33],
    )
    for _ in range(wrappers):
        value = [value]
    if wrappers == 62:
        middle = b"(" + b",".join([b"(1,'a')"] * 33) + b")"
        assert _sqlaero._encode_value(value).to_bytes() == b"(" * wrappers + middle + b")" * wrappers
    else:
        with pytest.raises(DataError) as caught:
            _sqlaero._encode_value(value)
        assert caught.value.code == 65543


def test_pandas_empty_multi_index_does_not_hide_unsupported_level_schema() -> None:
    value = pd.MultiIndex(levels=[pd.period_range("2024-01-01", periods=0, freq="D"), []], codes=[[], []])
    with pytest.raises(TypeError, match="declared Pandas extension dtype"):
        _sqlaero._bind_sql("SELECT 1", value)


@pytest.mark.parametrize("kind", ["time", "nested"])
def test_pandas_complex_multi_index_levels_keep_the_original_tuple_reader(kind: str) -> None:
    if kind == "time":
        level = pd.DatetimeIndex(["2024-01-02 03:04:05.123456789"], tz="Asia/Shanghai")
        literal = b"'2024-01-01 19:04:05.123456789'"
    else:
        level = pd.Index([(1, 2)], dtype=object, tupleize_cols=False)
        literal = b"(1,2)"
    value = pd.MultiIndex(levels=[level, pd.Index(["a"], dtype=object)], codes=[[0] * 33, [0] * 33])
    expected = b"\x03SELECT (" + literal + b",'a')"
    assert _sqlaero._bind_sql("SELECT %s", value, many=True) == [expected] * 33


@pytest.mark.parametrize("unit", ["s", "ms", "us", "ns"])
def test_pandas_aware_index_keeps_utc_instants_across_dst(unit: str) -> None:
    value = pd.DatetimeIndex(["2024-11-03T05:30:00Z", "2024-11-03T06:30:00Z", None]).tz_convert("America/New_York").as_unit(unit)
    assert _sqlaero._bind_sql("SELECT %s", value, many=True) == [
        b"\x03SELECT '2024-11-03 05:30:00'", b"\x03SELECT '2024-11-03 06:30:00'", b"\x03SELECT NULL",
    ]


def test_pandas_timedelta_index_retains_negative_nanoseconds() -> None:
    value = pd.TimedeltaIndex([pd.Timedelta(-1, unit="ns"), pd.NaT, pd.Timedelta(1, unit="ns")])
    assert _sqlaero._bind_sql("SELECT %s", value, many=True) == [
        b"\x03SELECT '-00:00:00.000000001'", b"\x03SELECT NULL", b"\x03SELECT '00:00:00.000000001'",
    ]


@pytest.mark.parametrize("many", [False, True])
@pytest.mark.parametrize("itemize", [False, True])
def test_pandas_dataframe_preserves_positions_and_duplicate_column_names(many: bool, itemize: bool) -> None:
    value = pd.DataFrame([[1, 2], [3, 4]], columns=["same", "same"], index=[object(), object()])
    sql = "SELECT %s,%s" if itemize else "SELECT %s"
    expected = [b"\x03SELECT 1,2", b"\x03SELECT 3,4"] if itemize else [b"\x03SELECT (1,2)", b"\x03SELECT (3,4)"]
    assert _sqlaero._bind_sql(sql, value, many=many, itemize=itemize) == expected


def test_pandas_dataframe_mixed_extension_storage_uses_one_row_order() -> None:
    value = pd.DataFrame({
        "integer": pd.array([1, None], dtype="Int64"),
        "text": pd.array(["a", None], dtype=pd.StringDtype(storage="python")),
        "flag": pd.array([None, True], dtype="boolean"),
        "nested": [(1, 2), [3, 4]],
    })
    assert _sqlaero._bind_sql("SELECT %s,%s,%s,%s", value) == [
        b"\x03SELECT 1,'a',NULL,(1,2)", b"\x03SELECT NULL,NULL,1,(3,4)",
    ]


@pytest.mark.parametrize("shape", [(0, 0), (0, 2), (2, 0)])
@pytest.mark.parametrize("many", [False, True])
@pytest.mark.parametrize("itemize", [False, True])
def test_pandas_empty_root_and_zero_column_rows_keep_the_unified_rule(shape: tuple[int, int], many: bool, itemize: bool) -> None:
    value = pd.DataFrame(np.zeros(shape, dtype=np.int64))
    count = max(shape[0], 1)
    assert _sqlaero._bind_sql("SELECT 1", value, many=many, itemize=itemize) == [b"\x03SELECT 1"] * count
    with pytest.raises(ProgrammingError):
        _sqlaero._bind_sql("SELECT %s", value, many=many, itemize=itemize)


@pytest.mark.parametrize("length", [0, 1])
@pytest.mark.parametrize("kind", ["period", "interval", "sparse"])
def test_pandas_unsupported_declared_extension_schema_is_not_hidden_by_empty_values(length: int, kind: str) -> None:
    if kind == "period":
        value = pd.Series(pd.period_range("2024-01-01", periods=length, freq="D"))
    elif kind == "interval":
        value = pd.Series(pd.arrays.IntervalArray.from_tuples([(0, 1)] * length))
    else:
        value = pd.Series(pd.arrays.SparseArray([0] * length))
    with pytest.raises(TypeError, match="declared Pandas extension dtype"):
        _sqlaero._encode_value(value)


def test_pandas_series_subclass_values_hook_runs_once_and_defines_values() -> None:
    calls: list[int] = []
    class Series(pd.Series):
        @property
        def values(self) -> Any:
            calls.append(1)
            return np.array([7, 8], dtype=np.int64)
    value = Series([1, 2])
    assert _sqlaero._bind_sql("SELECT %s", value, many=True) == [b"\x03SELECT 7", b"\x03SELECT 8"]
    assert calls == [1]


def test_pandas_series_subclass_original_values_exception_is_preserved() -> None:
    marker = RuntimeError("original values failure")
    class Series(pd.Series):
        @property
        def values(self):
            raise marker
    with pytest.raises(RuntimeError) as caught:
        _sqlaero._encode_value(Series([1]))
    assert caught.value is marker


@pytest.mark.parametrize("values", [np.zeros((2, 1)), np.array([1]), [1, 2]])
def test_pandas_series_subclass_values_are_shape_and_type_checked(values: object) -> None:
    class Series(pd.Series):
        @property
        def values(self):
            return values
    with pytest.raises(TypeError):
        _sqlaero._encode_value(Series([1, 2]))


def test_pandas_nullable_blocks_preserve_values_across_block_boundaries() -> None:
    value = pd.array([None if i % 17 == 0 else i for i in range(2051)], dtype="Int64")
    rows = _sqlaero._bind_sql("SELECT %s", value, many=True)
    assert rows == [b"\x03SELECT " + (b"NULL" if i % 17 == 0 else str(i).encode("ascii")) for i in range(2051)]


def test_pandas_object_back_reference_uses_the_original_series_identity() -> None:
    value = pd.Series([None], dtype=object)
    value.iloc[0] = value
    with pytest.raises(DataError) as caught:
        _sqlaero._encode_value(value)
    assert caught.value.code == 65543


def test_pandas_shared_stable_sources_and_cold_preparation_are_thread_safe() -> None:
    script = r"""
import concurrent.futures,sys,threading
before = getattr(sys, '_is_gil_enabled', lambda: True)()
import pandas as pd
from sqlaero import _sqlaero
assert getattr(sys, '_is_gil_enabled', lambda: True)() == before
value = pd.Series(pd.array([1,None,3],dtype='Int64'))
sources = [(value, [b'\x03SELECT 1',b'\x03SELECT NULL',b'\x03SELECT 3'])]
sources.append((
    pd.array([1,None,3]*12,dtype='Int64'),
    [b'\x03SELECT 1',b'\x03SELECT NULL',b'\x03SELECT 3']*12,
))
sources.append((
    pd.Categorical.from_codes([0,-1,1]*12, categories=pd.Index([7,9],dtype='int64')),
    [b'\x03SELECT 7',b'\x03SELECT NULL',b'\x03SELECT 9']*12,
))
sources.append((
    pd.Categorical.from_codes([0,-1,1]*172, categories=pd.Index(pd.array(['a','b'],dtype=pd.StringDtype(storage='python')))),
    [b"\x03SELECT 'a'",b'\x03SELECT NULL',b"\x03SELECT 'b'"]*172,
))
sources.append((
    pd.MultiIndex(levels=[pd.Index([7,9],dtype='int64'),pd.Index(['a','b'],dtype=object)],codes=[[0,1]*18,[0,1]*18]),
    [b"\x03SELECT (7,'a')",b"\x03SELECT (9,'b')"]*18,
))
barrier = threading.Barrier(4)
def run(index):
    barrier.wait(timeout=10)
    for _ in range(16):
        for source, expected in sources:
            assert _sqlaero._bind_sql('SELECT %s',source,many=True) == expected
    return index
with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
    assert sorted(pool.map(run,range(4))) == list(range(4))
assert getattr(sys, '_is_gil_enabled', lambda: True)() == before
"""
    result = subprocess.run([sys.executable, "-c", script], capture_output=True, text=True, timeout=30, check=False)
    assert result.returncode == 0, result.stdout + result.stderr


def test_pandas_arrow_to_pandas_zero_copy_numpy_columns_keep_the_native_owner() -> None:
    pa = pytest.importorskip("pyarrow")
    value = pa.table({"left": pa.array([1, 2]), "right": pa.array([3, 4])}).to_pandas()
    assert _sqlaero._bind_sql("SELECT %s,%s", value) == [b"\x03SELECT 1,3", b"\x03SELECT 2,4"]


def test_pandas_range_index_beyond_native_integer_width_uses_its_exact_values() -> None:
    start = 2**130
    value = pd.RangeIndex(start, start + 2)
    assert _sqlaero._bind_sql("SELECT %s", value, many=True) == [
        b"\x03SELECT " + str(start).encode("ascii"), b"\x03SELECT " + str(start + 1).encode("ascii"),
    ]


@pytest.mark.parametrize(("start", "stop", "step"), [
    pytest.param(2**63 - 2, 2**63 + 2, 1, id="cross-i64-upper"),
    pytest.param(-(2**63) + 1, -(2**63) - 3, -1, id="cross-i64-lower-descending"),
    pytest.param(2**127 - 2, 2**127, 1, id="i128-maximum"),
    pytest.param(-(2**127), -(2**127) + 2, 1, id="i128-minimum"),
    pytest.param(2**127 - 2, 2**127 + 4, 3, id="checked-add-overflow"),
    pytest.param(-(2**127) + 1, -(2**127) - 5, -3, id="checked-add-underflow"),
    pytest.param(-(2**127), 2**127 - 1, 2**127 - 1, id="checked-product-overflow-with-final-cancellation"),
    pytest.param(0, 2**131, 2**130, id="step-wider-than-i128"),
    pytest.param(2**130, 2**130, -1, id="empty-wide-start"),
])
def test_pandas_range_index_integer_boundaries_match_python_range(start: int, stop: int, step: int) -> None:
    value = pd.RangeIndex(start, stop, step)
    # Python range uses arbitrary-precision integer semantics, independent of
    # the driver's checked arithmetic and choice of native integer writer.
    expected_values = list(range(start, stop, step))
    tokens = [str(number).encode("ascii") for number in expected_values]
    if tokens:
        assert _sqlaero._bind_sql("SELECT %s", value, many=True) == [b"\x03SELECT " + token for token in tokens]
    else:
        assert _sqlaero._bind_sql("SELECT 1", value, many=True) == [b"\x03SELECT 1"]
        with pytest.raises(ProgrammingError):
            _sqlaero._bind_sql("SELECT %s", value, many=True)
    assert _sqlaero._encode_value(value).to_bytes() == b"(" + b",".join(tokens) + b")"


def test_pandas_dataframe_nested_as_a_cell_does_not_become_an_outer_batch() -> None:
    value = pd.DataFrame([[1, 2], [3, 4]])
    assert _sqlaero._bind_sql("SELECT %s", (value,)) == [b"\x03SELECT ((1,2),(3,4))"]


def test_pandas_basic_sources_do_not_require_pyarrow() -> None:
    script = r"""
import importlib.abc,sys
class NoArrow(importlib.abc.MetaPathFinder):
    def find_spec(self,fullname,path=None,target=None):
        if fullname == 'pyarrow' or fullname.startswith('pyarrow.'):
            raise ModuleNotFoundError('pyarrow deliberately unavailable',name=fullname)
sys.meta_path.insert(0,NoArrow())
import pandas as pd
from sqlaero import _sqlaero
assert 'pyarrow' not in sys.modules
assert _sqlaero._encode_value(pd.NA).to_bytes() == b'NULL'
for value in (pd.Series([1,2]),pd.Index([1,2]),pd.array([1,None],dtype='Int64')):
    expected = [b'\x03SELECT 1',b'\x03SELECT NULL'] if len(value) == 2 and value.dtype.name == 'Int64' else [b'\x03SELECT 1',b'\x03SELECT 2']
    assert _sqlaero._bind_sql('SELECT %s',value,many=True) == expected
text=pd.array(['a',None],dtype=pd.StringDtype(storage='python'))
assert _sqlaero._bind_sql('SELECT %s',text,many=True) == [b"\x03SELECT 'a'",b'\x03SELECT NULL']
assert 'pyarrow' not in sys.modules
"""
    result = subprocess.run([sys.executable, "-c", script], capture_output=True, text=True, timeout=30, check=False)
    assert result.returncode == 0, result.stdout + result.stderr


@pytest.mark.parametrize("state", ["absent", "unimported", "none", "loaded", "later"])
def test_pandas_nullable_arrow_bridge_keeps_optional_module_states(state: str) -> None:
    if state in ("loaded", "later"):
        pytest.importorskip("pyarrow")
    script = r"""
import builtins,importlib.abc,sys,types
state = sys.argv[1]
before = getattr(sys, '_is_gil_enabled', lambda: True)()
class NoArrow(importlib.abc.MetaPathFinder):
    def find_spec(self, fullname, path=None, target=None):
        if fullname == 'pyarrow' or fullname.startswith('pyarrow.'):
            raise ModuleNotFoundError('pyarrow intentionally unavailable during Pandas import',name=fullname)
blocker = NoArrow()
sys.meta_path.insert(0,blocker)
import pandas as pd
from sqlaero import _sqlaero
assert 'pyarrow' not in sys.modules
assert getattr(sys, '_is_gil_enabled', lambda: True)() == before
if state == 'none':
    sys.modules['pyarrow'] = None
elif state in ('unimported','later','loaded'):
    sys.meta_path.remove(blocker)
if state == 'loaded':
    import pyarrow
original_import = builtins.__import__
def guarded_import(name,*args,**kwargs):
    if (name == 'pyarrow' or name.startswith('pyarrow.')) and not isinstance(sys.modules.get('pyarrow'),types.ModuleType):
        raise AssertionError('Escape must not probe-import an optional provider')
    return original_import(name,*args,**kwargs)
builtins.__import__ = guarded_import
value = pd.array([1,None,3]*12,dtype='Int64')
expected = [b'\x03SELECT 1',b'\x03SELECT NULL',b'\x03SELECT 3']*12
assert _sqlaero._bind_sql('SELECT %s',value,many=True) == expected
if state == 'none':
    assert 'pyarrow' in sys.modules and sys.modules['pyarrow'] is None
elif state != 'loaded':
    assert 'pyarrow' not in sys.modules
if state == 'later':
    builtins.__import__ = original_import
    import pyarrow
    builtins.__import__ = guarded_import
    assert _sqlaero._bind_sql('SELECT %s',value,many=True) == expected
assert getattr(sys, '_is_gil_enabled', lambda: True)() == before
"""
    result = subprocess.run([sys.executable, "-c", script, state], capture_output=True, text=True, timeout=30, check=False)
    assert result.returncode == 0, result.stdout + result.stderr


@pytest.mark.parametrize("count", [1023, 1024, 1025])
def test_pandas_nullable_native_preparation_remains_bounded_to_current_piece(monkeypatch: pytest.MonkeyPatch, count: int) -> None:
    pytest.importorskip("pyarrow")
    value = pd.array([None if index % 17 == 0 else index for index in range(count)], dtype="Int64")
    lengths: list[int] = []
    original_arrow = pd.arrays.IntegerArray.__arrow_array__
    original_mask = pd.arrays.IntegerArray.isna
    def exported(array: Any, type: Any = None) -> Any:
        lengths.append(len(array))
        return original_arrow(array, type=type)
    def masked(array: Any) -> Any:
        lengths.append(len(array))
        return original_mask(array)
    monkeypatch.setattr(pd.arrays.IntegerArray, "__arrow_array__", exported)
    monkeypatch.setattr(pd.arrays.IntegerArray, "isna", masked)
    expected = [b"\x03SELECT " + (b"NULL" if index % 17 == 0 else str(index).encode()) for index in range(count)]
    assert _sqlaero._bind_sql("SELECT %s", value, many=True) == expected
    assert lengths == ([count] if count <= 1024 else [1024, 1])


@pytest.mark.parametrize("reverse", [False, True])
def test_pandas_nullable_boolean_strides_keep_data_and_mask_bit_boundaries(reverse: bool) -> None:
    pytest.importorskip("pyarrow")
    indices = np.arange(2062)
    value = pd.arrays.BooleanArray(indices % 3 == 0, indices % 7 == 0)[5:2055:2]
    positions = list(range(5, 2055, 2))
    if reverse:
        value = value[::-1]
        positions.reverse()
    expected = [
        b"\x03SELECT " + (b"NULL" if index % 7 == 0 else b"1" if index % 3 == 0 else b"0")
        for index in positions
    ]
    assert _sqlaero._bind_sql("SELECT %s", value, many=True) == expected


def test_pandas_nullable_unaligned_floating_storage_keeps_mask_and_exact_value() -> None:
    pytest.importorskip("pyarrow")
    backing = bytearray(36 * 8 + 1)
    data = np.ndarray((36,), dtype=np.float64, buffer=backing, offset=1)
    data[:] = np.arange(36) * 0.25
    value = pd.arrays.FloatingArray(data, np.arange(36) % 7 == 6)
    rows = _sqlaero._bind_sql("SELECT %s", value, many=True)
    for index, row in enumerate(rows):
        if index % 7 == 6:
            assert row == b"\x03SELECT NULL"
        else:
            assert float(row.removeprefix(b"\x03SELECT ")).hex() == (index * 0.25).hex()


@pytest.mark.parametrize("kind", ["integer", "floating", "boolean"])
def test_pandas_nullable_all_missing_blocks_never_use_valid_looking_payload(kind: str) -> None:
    pytest.importorskip("pyarrow")
    mask = np.ones(1025, dtype=np.bool_)
    if kind == "integer":
        value = pd.arrays.IntegerArray(np.full(1025, -123, dtype=np.int64), mask)
    elif kind == "floating":
        value = pd.arrays.FloatingArray(np.full(1025, 1.5, dtype=np.float64), mask)
    else:
        value = pd.arrays.BooleanArray(np.ones(1025, dtype=np.bool_), mask)
    assert _sqlaero._bind_sql("SELECT %s", value, many=True) == [b"\x03SELECT NULL"] * 1025


@pytest.mark.parametrize(("dtype", "small", "maximum", "expected"), [
    (np.float32, 2.0**-149, 3.4028234663852886e38, ["0x1.99999a0000000p-4", "-0x0.0p+0", "0x1.0000000000000p-149", "0x1.fffffe0000000p+127", None, None, None, None]),
    (np.float64, 5e-324, 1.7976931348623157e308, ["0x1.999999999999ap-4", "-0x0.0p+0", "0x0.0000000000001p-1022", "0x1.fffffffffffffp+1023", None, None, None, None]),
])
def test_pandas_nullable_native_float_keeps_subnormals_extremes_and_invalid_slots(dtype: type, small: float, maximum: float, expected: list[str | None]) -> None:
    pytest.importorskip("pyarrow")
    data = np.array([0.1, -0.0, small, maximum, float("nan"), float("inf"), -float("inf"), 9.0] * 5, dtype=dtype)
    mask = np.array([False] * 7 + [True], dtype=np.bool_)
    value = pd.arrays.FloatingArray(data, np.tile(mask, 5))
    rows = _sqlaero._bind_sql("SELECT %s", value, many=True)
    actual = [None if row == b"\x03SELECT NULL" else float(row.removeprefix(b"\x03SELECT ")).hex() for row in rows]
    assert actual == expected * 5


@pytest.mark.parametrize("error_type", [MemoryError, KeyboardInterrupt])
def test_pandas_nullable_preparation_keeps_original_error_without_fallback(monkeypatch: pytest.MonkeyPatch, error_type: type[BaseException]) -> None:
    pytest.importorskip("pyarrow")
    value = pd.array([1, None, 3] * 12, dtype="Int64")
    calls: list[str] = []
    original = error_type("original nullable block preparation")
    def exported(*args: Any, **kwargs: Any) -> Any:
        calls.append("arrow")
        raise original
    def masked(*args: Any, **kwargs: Any) -> Any:
        calls.append("mask")
        raise original
    monkeypatch.setattr(pd.arrays.IntegerArray, "__arrow_array__", exported)
    monkeypatch.setattr(pd.arrays.IntegerArray, "isna", masked)
    with pytest.raises(error_type) as caught:
        _sqlaero._bind_sql("SELECT %s", value, many=True)
    assert caught.value is original
    # I44 exercises mask; the candidate exercises Arrow. Either route makes
    # exactly one failed preparation, never a second try after a real error.
    assert len(calls) == 1


def test_pandas_nullable_late_block_error_keeps_original_cause_and_row(monkeypatch: pytest.MonkeyPatch) -> None:
    pytest.importorskip("pyarrow")
    value = pd.array(range(1025), dtype="Int64")
    calls: list[str] = []
    original = MemoryError("original late nullable block preparation")
    original_arrow = pd.arrays.IntegerArray.__arrow_array__
    original_mask = pd.arrays.IntegerArray.isna
    def exported(array: Any, type: Any = None) -> Any:
        calls.append("arrow")
        if len(calls) == 2:
            raise original
        return original_arrow(array, type=type)
    def masked(array: Any) -> Any:
        calls.append("mask")
        if len(calls) == 2:
            raise original
        return original_mask(array)
    monkeypatch.setattr(pd.arrays.IntegerArray, "__arrow_array__", exported)
    monkeypatch.setattr(pd.arrays.IntegerArray, "isna", masked)
    with pytest.raises(MemoryError) as caught:
        _sqlaero._bind_sql("SELECT %s", value, many=True)
    assert caught.value is original
    assert len(calls) == 2
    assert "args[1024]" in "\n".join(getattr(caught.value, "__notes__", []))


@pytest.mark.parametrize("fail", [False, True])
def test_pandas_nullable_real_array_subclass_keeps_its_mask_and_conversion_hooks(fail: bool) -> None:
    pytest.importorskip("pyarrow")
    calls: list[str] = []
    original = RuntimeError("original nullable subclass mask")
    class Array(pd.arrays.IntegerArray):
        def __arrow_array__(self, type: Any = None) -> Any:
            raise AssertionError("a real EA subclass must keep its existing path")
        def isna(self) -> Any:
            calls.append("mask")
            if fail:
                raise original
            parent: Any = super()
            return parent.isna()
        def to_numpy(self, *args: Any, **kwargs: Any) -> Any:
            calls.append("numpy")
            parent: Any = super()
            return parent.to_numpy(*args, **kwargs)
    value = pd.Series(Array(np.arange(36, dtype=np.int64), np.zeros(36, dtype=np.bool_)))
    assert type(value.array) is Array
    calls.clear()
    if fail:
        with pytest.raises(RuntimeError) as caught:
            _sqlaero._bind_sql("SELECT %s", value, many=True)
        assert caught.value is original
        assert calls == ["mask"]
    else:
        assert _sqlaero._bind_sql("SELECT %s", value, many=True) == [b"\x03SELECT " + str(index).encode() for index in range(36)]
        assert calls == ["mask", "numpy"]


def test_pandas_nullable_block_preparation_uses_the_original_operation_deadline(monkeypatch: pytest.MonkeyPatch) -> None:
    import time
    pytest.importorskip("pyarrow")
    value = pd.array(range(1025), dtype="Int64")
    # Prepare optional type identities before starting the tested budget.
    assert _sqlaero._encode_value(pd.array([1], dtype="Int64")).to_bytes() == b"(1)"
    calls: list[str] = []
    original_arrow = pd.arrays.IntegerArray.__arrow_array__
    original_mask = pd.arrays.IntegerArray.isna
    def exported(array: Any, type: Any = None) -> Any:
        calls.append("arrow")
        time.sleep(0.1)
        return original_arrow(array, type=type)
    def masked(array: Any) -> Any:
        calls.append("mask")
        time.sleep(0.1)
        return original_mask(array)
    monkeypatch.setattr(pd.arrays.IntegerArray, "__arrow_array__", exported)
    monkeypatch.setattr(pd.arrays.IntegerArray, "isna", masked)
    with pytest.raises(_sqlaero.OperationalTimeoutError) as caught:
        _sqlaero._bind_sql("SELECT %s", value, many=True, timeout_ns=50_000_000)
    assert caught.value.is_server_error is False
    assert len(calls) == 1


def test_pandas_nullable_native_blocks_keep_the_original_signal_exception() -> None:
    from datetime import datetime, timedelta, tzinfo
    from types import FrameType
    from _native_signal import NativeAlarm
    pytest.importorskip("pyarrow")
    value = pd.array(np.arange(100_000, dtype=np.int64), dtype="Int64")
    class InterruptedNullable(Exception):
        pass
    original = InterruptedNullable("original nullable block signal")
    interrupted = False
    def interrupt(signum: int, frame: FrameType | None) -> None:
        nonlocal interrupted
        if interrupted or frame is None or frame.f_code is not test_pandas_nullable_native_blocks_keep_the_original_signal_exception.__code__:
            return
        interrupted = True
        alarm.cancel()
        raise original
    class Arm(tzinfo):
        def utcoffset(self, value: datetime | None) -> timedelta:
            alarm.start(0.0001, 0.0001, wait_ready=True)
            return timedelta(0)
    alarm = NativeAlarm(interrupt)
    try:
        with pytest.raises(InterruptedNullable) as caught:
            _sqlaero._bind_sql("SELECT %s,%s", [datetime(2024, 1, 1, tzinfo=Arm()), value])
        assert caught.value is original
    finally:
        alarm.close()


def test_pandas_cold_preparation_failure_keeps_the_original_and_can_retry() -> None:
    script = r"""
import pandas as pd
from sqlaero import _sqlaero
value=pd.Series([7,8])
saved=pd.Int8Dtype
original=getattr(pd,'__getattr__',None)
marker=RuntimeError('original Pandas API preparation failure')
def missing(name):
    if name=='Int8Dtype': raise marker
    if original is not None: return original(name)
    raise AttributeError(name)
del pd.Int8Dtype
pd.__getattr__=missing
assert _sqlaero._encode_value(1).to_bytes()==b'1'
try:
    _sqlaero._bind_sql('SELECT %s',value,many=True)
except RuntimeError as error:
    assert error is marker
else:
    raise AssertionError('incomplete provider preparation must fail')
finally:
    pd.Int8Dtype=saved
    if original is None: del pd.__getattr__
    else: pd.__getattr__=original
assert _sqlaero._bind_sql('SELECT %s',value,many=True)==[b'\x03SELECT 7',b'\x03SELECT 8']
"""
    result = subprocess.run([sys.executable, "-c", script], capture_output=True, text=True, timeout=30, check=False)
    assert result.returncode == 0, result.stdout + result.stderr


@pytest.mark.parametrize("value", [
    pd.Series([1, 2]).array,
    pd.date_range("2024-01-01", periods=2).array,
    pd.timedelta_range("1 day", periods=2).array,
    pd.period_range("2024-01-01", periods=2, freq="D").array,
    pd.arrays.IntervalArray.from_tuples([(0, 1)]),
    pd.arrays.SparseArray([1, 2]),
])
def test_pandas_internal_or_excluded_array_classes_do_not_gain_standalone_entry(value: object) -> None:
    with pytest.raises(TypeError):
        _sqlaero._encode_value(value)


def test_pandas_active_subclass_cycle_is_rejected_before_repeating_values_hook() -> None:
    calls: list[int] = []
    storage = np.empty(1, dtype=object)
    class Series(pd.Series):
        @property
        def values(self) -> Any:
            calls.append(1)
            return storage
    value = Series([None], dtype=object)
    storage[0] = value
    assert storage[0] is value
    try:
        with pytest.raises(DataError) as caught:
            _sqlaero._encode_value(value)
        assert caught.value.code == 65543
        assert calls == [1]
    finally:
        storage[0] = None


@pytest.mark.parametrize("fail", [False, True])
def test_pandas_actual_index_subclass_honors_values_once_and_keeps_its_exception(fail: bool) -> None:
    calls: list[int] = []
    original = RuntimeError("original Index values conversion")
    class Index(pd.RangeIndex):
        @property
        def values(self) -> Any:
            calls.append(1)
            if fail:
                raise original
            return np.array([7, 8], dtype=np.int64)
    value = Index(2)
    assert type(value) is Index
    if fail:
        with pytest.raises(RuntimeError) as caught:
            _sqlaero._encode_value(value)
        assert caught.value is original
    else:
        assert _sqlaero._bind_sql("SELECT %s", value, many=True) == [b"\x03SELECT 7", b"\x03SELECT 8"]
    assert calls == [1]


def test_pandas_categorical_index_uses_labels_and_missing_instead_of_codes_or_names() -> None:
    value = pd.CategoricalIndex(["b", None, "a"], categories=["a", "b"], ordered=True, name=object())
    assert _sqlaero._bind_sql("SELECT %s", value, many=True) == [b"\x03SELECT 'b'", b"\x03SELECT NULL", b"\x03SELECT 'a'"]


@pytest.mark.parametrize("dtype", [np.float32, np.float64])
def test_pandas_floating_array_distinguishes_the_mask_from_valid_nonfinite_storage(dtype: type) -> None:
    value = pd.arrays.FloatingArray(
        np.array([float("nan"), float("inf"), -float("inf"), -0.0, 9.0], dtype=dtype),
        np.array([False, False, False, False, True], dtype=np.bool_),
    )
    requests = _sqlaero._bind_sql("SELECT %s", value, many=True)
    assert requests[:3] == [b"\x03SELECT NULL"] * 3
    assert float(requests[3].removeprefix(b"\x03SELECT ")).hex() == "-0x0.0p+0"
    assert requests[4] == b"\x03SELECT NULL"


@pytest.mark.parametrize("kind", ["category", "integer", "floating", "boolean", "python_string", "arrow_string", "arrow_array"])
def test_pandas_all_seven_direct_arrays_keep_empty_schema_and_all_missing_values(kind: str) -> None:
    def make(values: list[None]):
        if kind == "category":
            return pd.Categorical(values, categories=["unused"], ordered=True)
        if kind == "integer":
            return pd.array(values, dtype="Int64")
        if kind == "floating":
            return pd.array(values, dtype="Float64")
        if kind == "boolean":
            return pd.array(values, dtype="boolean")
        if kind in ("python_string", "arrow_string"):
            storage = "python" if kind == "python_string" else "pyarrow"
            if storage == "pyarrow":
                pytest.importorskip("pyarrow")
            return pd.array(values, dtype=pd.StringDtype(storage=storage))
        pa = pytest.importorskip("pyarrow")
        return pd.array(values, dtype=pd.ArrowDtype(pa.int64()))
    empty = make([])
    assert _sqlaero._bind_sql("SELECT 1", empty, many=True) == [b"\x03SELECT 1"]
    with pytest.raises(ProgrammingError):
        _sqlaero._bind_sql("SELECT %s", empty, many=True)
    assert _sqlaero._bind_sql("SELECT %s", make([None, None]), many=True) == [b"\x03SELECT NULL"] * 2


@pytest.mark.parametrize("code", [-2, 2])
def test_pandas_categorical_invalid_code_is_rejected_before_label_access(code: int) -> None:
    value = pd.Categorical.from_codes([code], categories=["a", "b"], validate=False)
    with pytest.raises(ValueError, match="categorical code"):
        _sqlaero._encode_value(value)


@pytest.mark.parametrize(("storage", "count"), [("numeric", 35), ("python", 35), ("python", 511), ("python", 512), ("pyarrow", 35)])
@pytest.mark.parametrize(("many", "itemize"), [(False, False), (False, True), (True, False), (True, True)])
def test_pandas_standard_categorical_labels_keep_all_four_binding_roles(storage: str, count: int, many: bool, itemize: bool) -> None:
    if storage == "numeric":
        labels = pd.Index(np.array([7, -9], dtype=np.int16))
        tokens = [b"7", b"-9"]
    else:
        if storage == "pyarrow":
            pytest.importorskip("pyarrow")
        labels = pd.Index(pd.array(["ascii", "中'"], dtype=pd.StringDtype(storage=storage)))
        tokens = [b"'ascii'", "'中\\''".encode()]
    codes = ([0, -1, 1, 0, 1] * ((count + 4) // 5))[:count]
    value = pd.Categorical.from_codes(codes, categories=labels)
    cells = [b"NULL" if code == -1 else tokens[code] for code in codes]
    if many:
        sql = "SELECT %s"
        expected = [b"\x03SELECT " + cell for cell in cells]
    elif itemize:
        sql = "SELECT " + ",".join(["%s"] * len(codes))
        expected = [b"\x03SELECT " + b",".join(cells)]
    else:
        sql = "SELECT %s"
        expected = [b"\x03SELECT (" + b",".join(cells) + b")"]
    assert _sqlaero._bind_sql(sql, value, many=many, itemize=itemize) == expected


@pytest.mark.parametrize("wrapper", ["series", "index", "frame"])
@pytest.mark.parametrize("unique", [False, True])
def test_pandas_standard_category_wrappers_keep_slice_null_and_cardinality(wrapper: str, unique: bool) -> None:
    count = 520
    labels = pd.Index(pd.array([f"k{i}" for i in range(count if unique else 3)], dtype=pd.StringDtype(storage="python")))
    codes = [-1 if i % 7 == 6 else i % len(labels) for i in range(count)]
    value = pd.Categorical.from_codes(codes, categories=labels)[::-1]
    cells = [b"NULL" if code == -1 else f"'k{code}'".encode() for code in codes[::-1]]
    if wrapper == "series":
        source = pd.Series(value, index=[object() for _ in range(count)])
        sql = "SELECT %s"
        expected = [b"\x03SELECT " + cell for cell in cells]
    elif wrapper == "index":
        source = pd.CategoricalIndex(value, name=object())
        sql = "SELECT %s"
        expected = [b"\x03SELECT " + cell for cell in cells]
    else:
        source = pd.DataFrame({f"c{column}": value for column in range(4)})
        sql = "SELECT %s,%s,%s,%s"
        expected = [b"\x03SELECT " + b",".join([cell] * 4) for cell in cells]
    assert _sqlaero._bind_sql(sql, source, many=True) == expected


def test_pandas_numeric_category_retains_uint64_width_and_float_bits() -> None:
    wide = pd.Categorical.from_codes([0, 1] * 17, categories=pd.Index(np.array([0, 2**64 - 1], dtype=np.uint64)))
    assert _sqlaero._bind_sql("SELECT %s", wide, many=True) == [b"\x03SELECT 0", b"\x03SELECT 18446744073709551615"] * 17
    labels = pd.Index(np.array([-0.0, 0.1, 1.7976931348623157e308, float("inf"), -float("inf")], dtype=np.float64))
    value = pd.Categorical.from_codes([0, 1, 2, 3, 4] * 7, categories=labels)
    rows = _sqlaero._bind_sql("SELECT %s", value, many=True)
    expected = ["-0x0.0p+0", "0x1.999999999999ap-4", "0x1.fffffffffffffp+1023", None, None] * 7
    actual = [None if row == b"\x03SELECT NULL" else float(row.removeprefix(b"\x03SELECT ")).hex() for row in rows]
    assert actual == expected


@pytest.mark.parametrize("storage", ["python", "pyarrow"])
@pytest.mark.parametrize("referenced", [False, True])
def test_pandas_category_only_encodes_referenced_native_string_labels(storage: str, referenced: bool) -> None:
    if storage == "pyarrow":
        pytest.importorskip("pyarrow")
    labels = pd.Index(pd.array(["ok", "🙂"], dtype=pd.StringDtype(storage=storage)))
    codes = [0] * 512 + ([1] if referenced else [0])
    value = pd.Categorical.from_codes(codes, categories=labels)
    if referenced:
        with pytest.raises(UnicodeEncodeError) as caught:
            _sqlaero._bind_sql("SELECT %s", value, many=True, charset="ascii")
        assert caught.value.object == "🙂"
        assert (caught.value.start, caught.value.end) == (0, 1)
        assert "args[512]" in "\n".join(getattr(caught.value, "__notes__", []))
    else:
        assert _sqlaero._bind_sql("SELECT %s", value, many=True, charset="ascii") == [b"\x03SELECT 'ok'"] * 513


@pytest.mark.parametrize("chunked", [False, True])
def test_pandas_arrow_category_nonmonotonic_codes_do_not_rewind_a_stream(chunked: bool) -> None:
    pa = pytest.importorskip("pyarrow")
    pieces = [["a"], [], ["b", "c"]] if chunked else [["a", "b", "c"]]
    storage = pd.arrays.ArrowStringArray(pa.chunked_array(pieces, type=pa.large_string()))
    labels = pd.Index(storage)
    value = pd.Categorical.from_codes([2, 0, 1, -1, 2, 0] * 6, categories=labels)
    expected = [b"\x03SELECT 'c'", b"\x03SELECT 'a'", b"\x03SELECT 'b'", b"\x03SELECT NULL", b"\x03SELECT 'c'", b"\x03SELECT 'a'"] * 6
    assert _sqlaero._bind_sql("SELECT %s", value, many=True) == expected


@pytest.mark.parametrize("fail", [False, True])
def test_pandas_object_category_keeps_every_real_label_conversion_and_original_error(fail: bool) -> None:
    calls: list[str] = []
    original = MemoryError("category original conversion")
    class Label(str):
        def __str__(self) -> str:
            calls.append("convert")
            if fail and len(calls) == 33:
                raise original
            return "wire"
    class Unused:
        def __repr__(self) -> str:
            raise AssertionError("unreferenced category must remain untouched")
    labels = pd.Index([Label("source"), Unused()], dtype=object)
    value = pd.Categorical.from_codes([0] * 35, categories=labels)
    calls.clear()
    if fail:
        with pytest.raises(MemoryError) as caught:
            _sqlaero._bind_sql("SELECT %s", value, many=True)
        assert caught.value is original
        assert calls == ["convert"] * 33
    else:
        assert _sqlaero._bind_sql("SELECT %s", value, many=True) == [b"\x03SELECT 'wire'"] * 35
        assert calls == ["convert"] * 35


@pytest.mark.parametrize("fail", [False, True])
def test_pandas_category_labels_real_index_subclass_keeps_getter_and_error(fail: bool) -> None:
    calls: list[int] = []
    original = KeyboardInterrupt("category label Index getter")
    class Labels(pd.RangeIndex):
        def __getitem__(self, key: Any) -> Any:
            calls.append(int(key))
            if fail and len(calls) == 33:
                raise original
            parent: Any = super()
            return parent.__getitem__(key)
    labels = Labels(3)
    value = pd.Categorical.from_codes([0, 1, 2] * 12, categories=labels)
    assert type(value.categories) is Labels
    calls.clear()
    if fail:
        with pytest.raises(KeyboardInterrupt) as caught:
            _sqlaero._bind_sql("SELECT %s", value, many=True)
        assert caught.value is original
        assert calls == ([0, 1, 2] * 11)
    else:
        assert _sqlaero._bind_sql("SELECT %s", value, many=True) == [b"\x03SELECT 0", b"\x03SELECT 1", b"\x03SELECT 2"] * 12
        assert calls == [0, 1, 2] * 12


@pytest.mark.parametrize("fail", [False, True])
def test_pandas_real_categorical_index_subclass_keeps_values_once(fail: bool) -> None:
    calls: list[int] = []
    original = MemoryError("CategoricalIndex values original")
    class Index(pd.CategoricalIndex):
        @property
        def values(self) -> Any:
            calls.append(1)
            if fail:
                raise original
            return np.full(35, 17, dtype=np.int64)
    value = Index(["ignored"] * 35)
    assert type(value) is Index
    if fail:
        with pytest.raises(MemoryError) as caught:
            _sqlaero._bind_sql("SELECT %s", value, many=True)
        assert caught.value is original
    else:
        assert _sqlaero._bind_sql("SELECT %s", value, many=True) == [b"\x03SELECT 17"] * 35
    assert calls == [1]


@pytest.mark.parametrize("first", [-2, -1])
def test_pandas_category_bad_code_or_all_nulls_do_not_request_label_array(monkeypatch: pytest.MonkeyPatch, first: int) -> None:
    value = pd.Categorical.from_codes([first] + [-1] * 34, categories=pd.Index([7, 9]), validate=False)
    original: Any = pd.Index.array
    target = value.categories
    def array(index: Any) -> Any:
        if index is target:
            raise AssertionError("no valid code requests this label storage")
        return original.__get__(index, type(index))
    monkeypatch.setattr(pd.Index, "array", property(array))
    if first == -2:
        with pytest.raises(ValueError, match="categorical code"):
            _sqlaero._bind_sql("SELECT %s", value, many=True)
    else:
        assert _sqlaero._bind_sql("SELECT %s", value, many=True) == [b"\x03SELECT NULL"] * 35


@pytest.mark.parametrize("returned", ["index", "category"])
def test_pandas_category_unexpected_public_label_array_falls_back_without_recursion(monkeypatch: pytest.MonkeyPatch, returned: str) -> None:
    value = pd.Categorical.from_codes([0, 1] * 17, categories=pd.Index([7, 9]))
    original: Any = pd.Index.array
    target = value.categories
    def array(index: Any) -> Any:
        if index is target:
            return target if returned == "index" else value
        return original.__get__(index, type(index))
    monkeypatch.setattr(pd.Index, "array", property(array))
    assert _sqlaero._bind_sql("SELECT %s", value, many=True) == [b"\x03SELECT 7", b"\x03SELECT 9"] * 17


def test_pandas_native_category_keeps_late_bad_code_and_output_limit() -> None:
    value = pd.Categorical.from_codes([0] * 34 + [2], categories=pd.Index([7, 9]), validate=False)
    with pytest.raises(ValueError, match="categorical code") as caught:
        _sqlaero._bind_sql("SELECT %s", value, many=True)
    assert "args[34]" in "\n".join(getattr(caught.value, "__notes__", []))
    valid = pd.Categorical.from_codes([0] * 35, categories=pd.Index([7, 9]))
    with pytest.raises(_sqlaero.OperationalError):
        _sqlaero._bind_sql("SELECT %s", (valid,), limit=len(b"\x03SELECT (7,7,7"))


@pytest.mark.parametrize("no_backslash", [False, True])
def test_pandas_category_native_text_uses_current_quote_context(no_backslash: bool) -> None:
    labels = pd.Index(pd.array(["A\\B'"], dtype=pd.StringDtype(storage="python")))
    value = pd.Categorical.from_codes([0] * 513, categories=labels)
    cell = b"'A\\B'''" if no_backslash else b"'A\\\\B\\''"
    assert _sqlaero._bind_sql("SELECT %s", value, many=True, no_backslash=no_backslash) == [b"\x03SELECT " + cell] * 513


@pytest.mark.parametrize("shape", [(1, 1), (1003, 3), (2, 33)])
def test_pandas_dataframe_basic_shapes_and_reversed_storage_keep_all_rows(shape: tuple[int, int]) -> None:
    data = np.arange(shape[0] * shape[1], dtype=np.int64).reshape(shape)
    value = pd.DataFrame(data, index=[object() for _ in range(shape[0])]).iloc[::-1]
    sql = "SELECT " + ",".join(["%s"] * shape[1])
    expected = [b"\x03SELECT " + b",".join(str(int(item)).encode("ascii") for item in row) for row in data[::-1]]
    assert _sqlaero._bind_sql(sql, value) == expected


def test_pandas_dataframe_arrow_columns_keep_distinct_chunk_boundaries_and_slices() -> None:
    pa = pytest.importorskip("pyarrow")
    left = pa.chunked_array([[1], [None, 3, 4]], type=pa.int64())
    right = pa.chunked_array([["a", "b"], [], ["c", "d"]], type=pa.string())
    value = pd.DataFrame({
        "left": pd.Series(left, dtype=pd.ArrowDtype(pa.int64())),
        "right": pd.Series(right, dtype=pd.ArrowDtype(pa.string())),
    })
    expected = [b"\x03SELECT 1,'a'", b"\x03SELECT NULL,'b'", b"\x03SELECT 3,'c'", b"\x03SELECT 4,'d'"]
    assert _sqlaero._bind_sql("SELECT %s,%s", value) == expected
    assert _sqlaero._bind_sql("SELECT %s,%s", value.iloc[1:]) == expected[1:]


def test_pandas_dataframe_subclasses_do_not_gain_an_unapproved_entry() -> None:
    class Frame(pd.DataFrame):
        pass
    with pytest.raises(TypeError):
        _sqlaero._encode_value(Frame([[1, 2]]))


def test_pandas_primitive_source_revalidates_before_reading_beyond_its_owned_run() -> None:
    script = r"""
from datetime import datetime,timedelta,tzinfo
import re
import numpy as np
import pandas as pd
from sqlaero import _sqlaero
calls=[]
armed=False
value=pd.DataFrame({'number':pd.Series(np.arange(2050,dtype='i8'),copy=False),
                    'callback':pd.Series([None]*2050,dtype=object)},copy=False)
root=value['number'].to_numpy(copy=False)
while isinstance(root.base,np.ndarray): root=root.base
assert root.flags.owndata
class Zone(tzinfo):
    def utcoffset(self,dt):
        if not armed: return timedelta(0)
        calls.append(1)
        root.__setstate__((1,(1,),np.dtype('i8'),False,bytes(8)))
        return timedelta(0)
value.iloc[0,1]=datetime(2024,1,1,tzinfo=Zone())
calls.clear()
armed=True
try:
    _sqlaero._bind_sql('SELECT %s,%s',value)
except BufferError as error:
    assert 'stable' in str(error) or 'physical allocation' in str(error)
    notes='\n'.join(getattr(error,'__notes__',()))
    positions=re.findall(r'args\[(\d+)\]\[0\]',notes)
    assert positions and 1<=int(positions[-1])<=1024,notes
else:
    raise AssertionError('a new read must reject the replaced allocation')
assert calls==[1]
"""
    result = subprocess.run([sys.executable, "-c", script], capture_output=True, text=True, timeout=30, check=False)
    assert result.returncode == 0, result.stdout + result.stderr
