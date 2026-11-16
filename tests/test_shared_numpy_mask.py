"""Actual mask facts precede payload reads and never become fill values."""

# pyright: reportPrivateUsage=false


from datetime import datetime, tzinfo
import json
import math
from typing import NoReturn

import pytest

from sqlaero import _sqlaero, json_dumps

np = pytest.importorskip("numpy")


@pytest.mark.parametrize("provider", ["numpy", "pandas-array", "pandas-series"])
@pytest.mark.parametrize("length", [31, 32, 33, 1023, 1024, 1025])
@pytest.mark.parametrize("layout", ["contiguous", "reversed", "stepped"])
def test_noncanonical_bool_masks_keep_actual_truth_across_owned_reads(
    provider: str, length: int, layout: str,
) -> None:
    # NumPy bool storage can contain any nonzero byte. Use a read-only root
    # and retain those actual bytes instead of normalizing through Python bool.
    pattern = (0, 2, 0, 255, 0, 128, 1)
    raw_mask = bytes(pattern[index % len(pattern)] for index in range(length))
    missing = np.frombuffer(raw_mask, dtype=bool)
    values = np.arange(length, dtype="i8")
    if provider == "numpy":
        source = np.ma.array(values, mask=False)
    else:
        pd = pytest.importorskip("pandas")
        source = pd.arrays.IntegerArray(values, np.zeros(length, dtype=bool))
    source.__dict__["_mask"] = missing
    if provider == "pandas-series":
        source = pd.Series(source)
    selection = slice(None, None, -1 if layout == "reversed" else 2 if layout == "stepped" else 1)
    source = source[selection]
    expected = [None if raw_mask[index] else index for index in range(length)[selection]]
    sql = b"(" + b",".join(b"NULL" if value is None else str(value).encode("ascii") for value in expected) + b")"
    assert _sqlaero._encode_value(source).to_bytes() == sql
    assert json_dumps(source) == json.dumps(expected, separators=(",", ":")).encode("ascii")


@pytest.mark.parametrize("provider", ["numpy", "pandas-array", "pandas-series"])
@pytest.mark.parametrize("length", [0, 1, 31, 32, 127, 128, 129, 255, 256, 257, 1023, 1024, 1025, 2051])
@pytest.mark.parametrize("layout", ["contiguous", "reversed", "stepped"])
def test_mask_values_across_owned_block_boundaries(provider: str, length: int, layout: str) -> None:
    values = np.arange(length, dtype="i8")
    missing = np.array([index % 3 == 1 for index in range(length)], dtype=bool)
    if provider == "numpy":
        source = np.ma.array(values, mask=missing)
    else:
        pd = pytest.importorskip("pandas")
        source = pd.arrays.IntegerArray(values, missing)
        if provider == "pandas-series":
            source = pd.Series(source)
    selection = slice(None, None, -1 if layout == "reversed" else 2 if layout == "stepped" else 1)
    source = source[selection]
    expected = [None if index % 3 == 1 else index for index in range(length)[selection]]
    sql = b"(" + b",".join(b"NULL" if value is None else str(value).encode("ascii") for value in expected) + b")"
    assert _sqlaero._encode_value(source).to_bytes() == sql
    assert json_dumps(source) == json.dumps(expected, separators=(",", ":")).encode("ascii")


@pytest.mark.parametrize("rows", [16, 32, 513, 1025])
@pytest.mark.parametrize("transpose", [False, True])
@pytest.mark.parametrize("reverse", [False, True])
def test_mask_table_coordinates_across_owned_block_boundaries(rows: int, transpose: bool, reverse: bool) -> None:
    data = np.arange(rows * 2, dtype="i8").reshape(rows, 2)
    missing = np.array([[(row + column) % 3 == 0 for column in range(2)] for row in range(rows)])
    source = np.ma.array(data, mask=missing)
    expected = [[None if (row + column) % 3 == 0 else row * 2 + column for column in range(2)] for row in range(rows)]
    if reverse:
        source = source[::-1]
        expected = expected[::-1]
    if transpose:
        source = source.T
        expected = [list(column) for column in zip(*expected)]
    sql_rows = [b"(" + b",".join(b"NULL" if value is None else str(value).encode("ascii") for value in row) + b")" for row in expected]
    assert _sqlaero._encode_value(source).to_bytes() == b"(" + b",".join(sql_rows) + b")"
    assert json_dumps(source) == json.dumps(expected, separators=(",", ":")).encode("ascii")


@pytest.mark.parametrize(("value", "sql", "document"), [
    (np.ma.array([7, 999, 8], mask=[False, True, False]), b"(7,NULL,8)", b"[7,null,8]"),
    (np.ma.array([7, 8], mask=np.ma.nomask), b"(7,8)", b"[7,8]"),
    (np.ma.array([7, 8], mask=[True, True]), b"(NULL,NULL)", b"[null,null]"),
    (np.ma.array(7, mask=False), b"7", b"7"),
    (np.ma.array(7, mask=True), b"NULL", b"null"),
    (np.ma.masked, b"NULL", b"null"),
    (np.ma.array([], dtype="i8"), b"()", b"[]"),
    (np.ma.array([math.nan, 0.5], mask=[True, False]), b"(NULL,0.5e0)", b"[null,0.5]"),
    (np.ma.array([[7, 8], [9, 10]], mask=[[False, True], [True, False]]),
     b"((7,NULL),(NULL,10))", b"[[7,null],[null,10]]"),
    (np.ma.array([object(), 7], dtype=object, mask=[True, False]), b"(NULL,7)", b"[null,7]"),
    (np.ma.array(["ab", "cd"], mask=[False, True]), b"('ab',NULL)", b'["ab",null]'),
    (np.ma.array([np.datetime64(-1, "ns"), np.datetime64("NaT", "ns")], mask=[False, True]),
     b"('1969-12-31 23:59:59.999999',NULL)", b'["1969-12-31T23:59:59.999999999",null]'),
])
def test_masked_array_values_follow_data_axes_and_actual_mask(value: object, sql: bytes, document: bytes) -> None:
    assert _sqlaero._encode_value(value).to_bytes() == sql
    assert json_dumps(value, allow_nan=False) == document


@pytest.mark.parametrize("fill", [0, -999, 123456])
def test_fill_value_has_no_effect(fill: int) -> None:
    value = np.ma.array([7, 999], mask=[False, True], fill_value=fill)
    assert json_dumps(value) == b"[7,null]"
    assert _sqlaero._encode_value(value).to_bytes() == b"(7,NULL)"


def test_masked_payload_protocol_is_never_called() -> None:
    original = MemoryError("unmasked timezone protocol")

    class Zone(tzinfo):
        def utcoffset(self, value: datetime | None) -> NoReturn:
            raise original

    value = np.ma.array([datetime(2026, 1, 2, tzinfo=Zone())], dtype=object, mask=[True])
    assert json_dumps(value) == b"[null]"
    assert _sqlaero._encode_value(value).to_bytes() == b"(NULL)"
    value.mask = [False]
    with pytest.raises(MemoryError) as caught:
        json_dumps(value)
    assert caught.value is original


@pytest.mark.parametrize("layout", ["reverse", "transpose", "broadcast"])
def test_data_and_mask_each_follow_actual_strides(layout: str) -> None:
    source = np.ma.array([[7, 8], [9, 10]], mask=[[False, True], [True, False]])
    if layout == "reverse":
        value = source[:, ::-1]
        expected = b"[[null,7],[10,null]]"
    elif layout == "transpose":
        value = source.T
        expected = b"[[7,null],[null,10]]"
    else:
        value = np.ma.array(np.broadcast_to(np.array([7, 8]), (2, 2)), mask=np.broadcast_to(np.array([False, True]), (2, 2)))
        expected = b"[[7,null],[7,null]]"
    assert json_dumps(value) == expected


def test_real_masked_subclass_ignores_replacement_storage_properties() -> None:
    calls: list[str] = []

    class Child(np.ma.MaskedArray):
        pass

    value = np.ma.array([7, 8], mask=[False, True]).view(Child)

    def forbidden(*args: object, **kwargs: object) -> object:
        calls.append("override")
        raise AssertionError("masked base storage replacement hook called")

    for name in ["__getattribute__", "__getitem__", "__iter__", "view", "filled"]:
        setattr(Child, name, forbidden)
    for name in ["mask", "data", "_mask", "dtype", "base"]:
        setattr(Child, name, property(forbidden))
    assert json_dumps(value) == b"[7,null]"
    assert _sqlaero._encode_value(value).to_bytes() == b"(7,NULL)"
    assert calls == []


@pytest.mark.parametrize("empty", [False, True])
def test_binary_schema_remains_rejected_by_json_when_empty_or_fully_masked(empty: bool) -> None:
    value = np.ma.array([], dtype="S2") if empty else np.ma.array([b"ab"], mask=[True])
    with pytest.raises(TypeError, match="binary"):
        json_dumps(value)
    assert _sqlaero._encode_value(value).to_bytes() == (b"()" if empty else b"(NULL)")


def test_mask_does_not_admit_unknown_schema_or_unmasked_value() -> None:
    with pytest.raises(TypeError):
        json_dumps(np.ma.array([1j], mask=[True]))
    with pytest.raises(TypeError):
        json_dumps(np.ma.array([object()], dtype=object, mask=[False]))
    with pytest.raises(ValueError):
        json_dumps(np.ma.array([math.nan], mask=[False]), allow_nan=False)


def test_masked_higher_axes_are_json_only() -> None:
    value = np.ma.array(np.arange(8).reshape(2, 2, 2), mask=False)
    value.mask[1, 0, 1] = True
    assert json_dumps(value) == b"[[[0,1],[2,3]],[[4,null],[6,7]]]"
    with pytest.raises(ValueError, match="axes"):
        _sqlaero._encode_value(value)


@pytest.mark.parametrize("length", [128, 129, 1024, 1025])
@pytest.mark.parametrize("pattern", ["none", "all", "alternating"])
@pytest.mark.parametrize("dtype", ["<i8", ">i8"])
def test_native_cached_run_preserves_masks_and_byte_order(length: int, pattern: str, dtype: str) -> None:
    values = np.arange(length, dtype=dtype)
    missing = np.array([pattern == "all" or pattern == "alternating" and index % 2 == 0 for index in range(length)])
    value = np.ma.array(values, mask=missing)
    expected = [None if flag else index for index, flag in enumerate(missing)]
    sql = b"(" + b",".join(b"NULL" if item is None else str(item).encode("ascii") for item in expected) + b")"
    assert _sqlaero._encode_value(value).to_bytes() == sql
    assert json_dumps(value) == json.dumps(expected, separators=(",", ":")).encode("ascii")


@pytest.mark.parametrize("provider", ["numpy", "pandas-array", "pandas-series"])
@pytest.mark.parametrize("position", [128, 129, 257, 1025])
def test_cached_numeric_failure_keeps_the_actual_json_index(provider: str, position: int) -> None:
    values = np.arange(1200, dtype="f8")
    values[position] = np.nan
    missing = np.array([index % 3 == 1 for index in range(1200)])
    if provider == "numpy":
        source = np.ma.array(values, mask=missing)
    else:
        pd = pytest.importorskip("pandas")
        source = pd.arrays.FloatingArray(values, missing)
        if provider == "pandas-series":
            source = pd.Series(source)
    with pytest.raises(ValueError, match="nonfinite") as caught:
        json_dumps(source, allow_nan=False)
    assert any(f"data[{position}]" in note for note in getattr(caught.value, "__notes__", []))
