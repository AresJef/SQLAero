"""Shared Arrow facts reach native targets without Python scalar conversion."""

# pyright: reportPrivateUsage=false


from typing import Any
from collections.abc import Callable

import json
import math

import pytest

from sqlaero import _sqlaero, json_dumps

pa = pytest.importorskip("pyarrow")


@pytest.mark.parametrize("kind", [pa.int8, pa.int16, pa.int32, pa.int64, pa.uint8, pa.uint16, pa.uint32, pa.uint64])
@pytest.mark.parametrize("chunked", [False, True])
def test_native_integer_values_and_masks(kind: Callable[[], Any], chunked: bool) -> None:
    value = pa.array([7, None, 8], type=kind())
    if chunked:
        value = pa.chunked_array([value.slice(0, 1), value.slice(1, 0), value.slice(1)])
    assert json_dumps(value) == b"[7,null,8]"
    assert _sqlaero._escape_projection(value) == ("7", "NULL", "8")


@pytest.mark.parametrize("kind", [pa.float32, pa.float64])
def test_native_float_bits_and_nonfinite_policy(kind: Callable[[], Any]) -> None:
    value = pa.array([-0.0, 0.5, None, float("nan"), float("inf")], type=kind())
    result = json.loads(json_dumps(value))
    assert math.copysign(1.0, result[0]) == -1.0
    assert result[1:] == [0.5, None, None, None]
    with pytest.raises(ValueError, match="nonfinite"):
        json_dumps(value, allow_nan=False)


@pytest.mark.parametrize("kind", [pa.string, pa.large_string, pa.string_view])
@pytest.mark.parametrize("length", [0, 7, 40000])
def test_native_utf8_complete_values_and_sliced_offsets(kind: Callable[[], Any], length: int) -> None:
    text = "ab\0\n\"\\\U0001f642" * length
    value = pa.array(["ignored", text, None, "tail"], type=kind()).slice(1, 2)
    assert json.loads(json_dumps(value)) == [text, None]


@pytest.mark.parametrize("kind", [pa.binary, pa.large_binary, pa.binary_view, lambda: pa.binary(2)])
def test_binary_schema_is_rejected_even_when_no_actual_value(kind: Callable[[], Any]) -> None:
    for value in (pa.array([], type=kind()), pa.array([None], type=kind())):
        with pytest.raises(TypeError, match="binary"):
            json_dumps(value)


def test_boolean_native_offsets_and_null_data_bits() -> None:
    value = pa.array([False, True, None, False, True]).slice(1, 3)
    assert json_dumps(value) == b"[true,null,false]"


def test_default_arrow_string_pandas_column_uses_shared_reader() -> None:
    pd = pytest.importorskip("pandas")
    value = pd.DataFrame({"id": [7, 8], "name": ["ab", "cd"]})
    assert json_dumps(value) == b'[{"id":7,"name":"ab"},{"id":8,"name":"cd"}]'
