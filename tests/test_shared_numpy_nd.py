"""Higher native array axes are JSON structure, never implicit SQL rows."""

# pyright: reportPrivateUsage=false


import json
import math

import pytest
from sqlaero import Cursor, DictCursor, SSCursor, SSDictCursor, _sqlaero, json_dumps
from test_public_connection import live, requires_mysql

np = pytest.importorskip("numpy")


@pytest.mark.parametrize("shape", [(2, 2, 2), (2, 1, 3, 2), (2, 0, 3), (0, 2, 3), (2, 3, 0), (1,) * 64])
@pytest.mark.parametrize("dtype", ["i8", ">i8", "f8", ">f8", "U8", ">U8", "O"])
def test_native_axes_and_empty_axes_are_preserved(shape: tuple[int, ...], dtype: str) -> None:
    source = np.arange(math.prod(shape), dtype="i8").reshape(shape).astype(dtype)
    assert json.loads(json_dumps(source)) == source.tolist()
    for convert in [_sqlaero._encode_value, _sqlaero._escape_projection]:
        with pytest.raises((TypeError, ValueError), match="dimension|axes"):
            convert(source)


@pytest.mark.parametrize("layout", ["reverse", "transpose", "broadcast", "strided"])
def test_noncontiguous_axes_reuse_the_actual_physical_owner(layout: str) -> None:
    original = np.arange(120, dtype="i8").reshape(4, 5, 6)
    source = (original[::-1, :, ::-1] if layout == "reverse" else original.transpose(2, 0, 1)
              if layout == "transpose" else np.broadcast_to(original[:1, :1, :], (3, 2, 6))
              if layout == "broadcast" else original[::2, ::2, ::2])
    assert json.loads(json_dumps(source)) == source.tolist()


@pytest.mark.parametrize("dtype", ["i8", "O"])
def test_three_axes_real_subclass_uses_qualified_base_storage(dtype: str) -> None:
    class Child(np.ndarray):
        pass
    source = np.arange(8, dtype="i8").reshape(2, 2, 2).astype(dtype).view(Child)
    def forbidden(*_args: object, **_kwargs: object):
        raise AssertionError("must not invoke source overrides")
    for name in ["__getitem__", "__iter__", "__buffer__", "__release_buffer__", "view", "tolist"]:
        setattr(Child, name, forbidden)
    for name in ["dtype", "shape", "strides", "ndim", "base", "flags"]:
        setattr(Child, name, property(forbidden))
    Child.__getattribute__ = forbidden
    assert json_dumps(source) == b"[[[0,1],[2,3]],[[4,5],[6,7]]]"


def test_object_axes_keep_values_location_cycles_and_actual_output_depth() -> None:
    source = np.empty((1, 1, 2), dtype="O")
    source[0, 0, 0] = {"id": 7}
    source[0, 0, 1] = [8, None]
    assert json_dumps(source) == b'[[[{"id":7},[8,null]]]]'
    source[0, 0, 1] = object()
    with pytest.raises(TypeError) as caught:
        json_dumps(source)
    assert "data[0][0][1]" in "\n".join(caught.value.__notes__)
    source[0, 0, 1] = source
    with pytest.raises(ValueError, match="cycle"):
        json_dumps(source)
    deep = np.empty((1,) * 64, dtype="O")
    # NumPy2.5.4's flat iterator still rejects rank>32; direct full coordinates
    # construct this supported64-axis Object fixture without that iterator.
    deep[(0,) * 64] = []
    with pytest.raises(ValueError, match="64"):
        json_dumps(deep)
    # Shape rank is not an invented output depth when an early axis is empty.
    assert json_dumps(np.empty((0,) + (1,) * 63, dtype="O")) == b"[]"


def test_native_temporal_axes_and_missing_values_keep_their_scalar_policy() -> None:
    source = np.array([0, -1, 1_234_567_890, -(1 << 63)], dtype="i8").view("M8[ns]").reshape(1, 2, 2)
    assert json_dumps(source, allow_nan=False) == b'[[["1970-01-01T00:00:00","1969-12-31T23:59:59.999999999"],["1970-01-01T00:00:01.234567890",null]]]'
    missing = np.full((2, 1, 2), -(1 << 63), dtype="i8").view("m8[ps]")
    assert json_dumps(missing, allow_nan=False) == b"[[[null,null]],[[null,null]]]"


def test_unsafe_strided_coordinate_is_rejected_before_native_payload_read() -> None:
    source = np.lib.stride_tricks.as_strided(np.arange(8, dtype="i8"),
                                           shape=(2, 2, 2), strides=(8, 8, 1 << 62))
    with pytest.raises((BufferError, ValueError)) as caught:
        json_dumps(source)
    assert "data[0][0][1]" in "\n".join(caught.value.__notes__)


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("kind", [Cursor, DictCursor])
@pytest.mark.parametrize("prepared", [False, True])
def test_sql_rejects_higher_axes_locally_and_keeps_the_old_trusted_result(label: str, kind: type[Cursor] | type[DictCursor] | type[SSCursor] | type[SSDictCursor], prepared: bool) -> None:
    with live(label) as connection, connection.cursor(kind, prepared=prepared) as cursor:
        cursor.execute("SELECT 7 AS value")
        old_columns = cursor.columns
        with pytest.raises((TypeError, ValueError), match="dimension|axes"):
            cursor.execute("SELECT %s", (np.zeros((1, 1, 1), dtype="i8"),))
        assert cursor.row_count == 1 and cursor.columns is old_columns
        assert cursor.fetchone() == ({"value": 7} if kind is DictCursor else (7,))
