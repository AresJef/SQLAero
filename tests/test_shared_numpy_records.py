"""Field facts are shared, while targets own record shape and value policy."""

# pyright: reportPrivateUsage=false


from datetime import datetime, timezone
import json
from typing import NoReturn

import pytest

from sqlaero import _sqlaero, json_dumps

np = pytest.importorskip("numpy")


def record(kind: str, dtype: object, values: tuple[object, ...]) -> object:
    source = np.array([values], dtype=dtype)
    if kind == "record":
        return source.view(np.recarray)[0]
    if kind == "scalar":
        return source[0]
    if kind == "zero_array":
        return source.reshape(())
    raise AssertionError(kind)


@pytest.mark.parametrize("kind", ["record", "scalar", "zero_array"])
@pytest.mark.parametrize(("dtype", "values", "sql", "document"), [
    ([("id", "i8"), ("name", "U2")], (7, "ab"), b"(7,'ab')", b'{"id":7,"name":"ab"}'),
    ([("a", ">i8"), ("b", "<f4")], (-7, 0.5), b"(-7,0.5e0)", b'{"a":-7,"b":0.5}'),
    ([("flag", "?")], (True,), b"(1)", b'{"flag":true}'),
    ([("at", "M8[ns]"), ("lag", "m8[ns]")],
     (np.datetime64(-1, "ns"), np.timedelta64(-1, "ns")),
     b"('1969-12-31 23:59:59.999999','00:00:00')",
     b'{"at":"1969-12-31T23:59:59.999999999","lag":"-PT0.000000001S"}'),
    ([("absent", "M8[ps]")], (np.datetime64("NaT", "ps"),), b"(NULL)", b'{"absent":null}'),
    ([("xy", "i8", (2,))], ([1, 2],), b"((1,2))", b'{"xy":[1,2]}'),
    ([("child", [("x", "i8"), ("y", "i8")])], ((1, 2),), b"((1,2))", b'{"child":{"x":1,"y":2}}'),
    ([("item", object)], ({"x": [1, 2]},), b"(((1,2)))", b'{"item":{"x":[1,2]}}'),
])
def test_one_record_uses_native_fields_and_target_layout(
    kind: str, dtype: object, values: tuple[object, ...], sql: bytes, document: bytes,
) -> None:
    value = record(kind, dtype, values)
    assert _sqlaero._encode_value(value).to_bytes() == sql
    assert json_dumps(value) == document


@pytest.mark.parametrize("kind", ["record", "scalar", "zero_array"])
def test_zero_field_record_remains_an_actual_object(kind: str) -> None:
    value = record(kind, [], ())
    assert _sqlaero._encode_value(value).to_bytes() == b"()"
    assert json_dumps(value) == b"{}"


def test_full_field_names_include_nul_suffix_and_title_alias_is_not_a_field() -> None:
    value = record("scalar", [(("title", "a\0x"), "i8"), ("a\0y", "i8")], (7, 8))
    assert json_dumps(value) == b'{"a\\u0000x":7,"a\\u0000y":8}'
    assert _sqlaero._encode_value(value).to_bytes() == b"(7,8)"


@pytest.mark.parametrize("kind", ["void", "record", "array"])
def test_real_record_subclasses_read_base_storage_without_replacement_hooks(kind: str) -> None:
    base = {"void": np.void, "record": np.record, "array": np.recarray}[kind]
    calls: list[str] = []

    class Child(base):
        pass

    source = np.array([(7, "ab")], dtype=[("id", "i8"), ("name", "U2")])
    value = source.view(Child).reshape(()) if kind == "array" else source.view(np.dtype((Child, source.dtype)))[0]

    def forbidden(*args: object, **kwargs: object) -> object:
        calls.append("override")
        raise AssertionError("record replacement hook was called")

    for name in ["__array__", "__getitem__", "__iter__", "__getattribute__", "view"]:
        setattr(Child, name, forbidden)
    for name in ["dtype", "base", "shape", "__array_interface__"]:
        setattr(Child, name, property(forbidden))
    assert json_dumps(value) == b'{"id":7,"name":"ab"}'
    assert _sqlaero._encode_value(value).to_bytes() == b"(7,'ab')"
    assert calls == []


@pytest.mark.parametrize("dtype", [[("bad", "c16")], [("bad", "V2")]])
def test_unknown_record_schema_is_rejected_even_without_value_rows(dtype: object) -> None:
    for value in [np.zeros((), dtype=dtype), np.empty(0, dtype=dtype)]:
        with pytest.raises(TypeError):
            json_dumps(value)
        with pytest.raises(TypeError):
            _sqlaero._encode_value(value)


def test_record_binary_schema_is_not_hidden_by_empty_storage() -> None:
    with pytest.raises(TypeError, match="binary"):
        json_dumps(np.empty(0, dtype=[("binary", "S2")]))


def test_record_object_cycle_and_repeated_noncyclic_children() -> None:
    source = np.empty(1, dtype=[("left", object), ("right", object)])
    shared = [1, 2]
    source[0] = (shared, shared)
    assert json_dumps(source[0]) == b'{"left":[1,2],"right":[1,2]}'
    source["left"][0] = source
    with pytest.raises(ValueError, match="cycle"):
        json_dumps(source)


def test_record_original_callback_error_and_field_location_are_preserved() -> None:
    from datetime import tzinfo

    original = MemoryError("record timezone source failed")

    class Zone(tzinfo):
        def utcoffset(self, value: datetime | None) -> NoReturn:
            raise original

    source = np.empty(1, dtype=[("id", "i8"), ("at", object)])
    source[0] = (7, datetime(2026, 11, 12, tzinfo=Zone()))
    with pytest.raises(MemoryError) as caught:
        json_dumps(source[0])
    assert caught.value is original
    assert 'at' in "\n".join(getattr(caught.value, "__notes__", ()))
    source["at"][0] = datetime(2026, 11, 12, tzinfo=timezone.utc)
    assert json.loads(json_dumps(source[0]))["at"] == "2026-11-12T00:00:00+00:00"


@pytest.mark.parametrize("recarray", [False, True])
@pytest.mark.parametrize("shape", [(2,), (1, 2), (2, 1, 1), (1, 0, 2)])
def test_record_arrays_preserve_axes_and_fields(recarray: bool, shape: tuple[int, ...]) -> None:
    source = np.array([(7, "ab"), (8, "cd")], dtype=[("id", "i8"), ("name", "U2")])
    if 0 in shape:
        source = source[:0]
    source = source.reshape(shape)
    if recarray:
        source = source.view(np.recarray)
    expected: object
    if shape == (2,):
        expected = [{"id": 7, "name": "ab"}, {"id": 8, "name": "cd"}]
    elif shape == (1, 2):
        expected = [[{"id": 7, "name": "ab"}, {"id": 8, "name": "cd"}]]
    elif shape == (2, 1, 1):
        expected = [[[{"id": 7, "name": "ab"}]], [[{"id": 8, "name": "cd"}]]]
    else:
        expected = [list[object]()]
    assert json.loads(json_dumps(source)) == expected
    if len(shape) >= 2:
        with pytest.raises(ValueError, match="axes"):
            _sqlaero._encode_value(source)


def test_record_array_nested_and_subarray_fields_are_one_field_each() -> None:
    dtype = [("id", "i8"), ("xy", "i8", (2,)), ("child", [("x", "i8"), ("y", "U2")])]
    source = np.array([(7, [1, 2], (3, "ab")), (8, [4, 5], (6, "cd"))], dtype=dtype)
    assert json.loads(json_dumps(source)) == [
        {"id": 7, "xy": [1, 2], "child": {"x": 3, "y": "ab"}},
        {"id": 8, "xy": [4, 5], "child": {"x": 6, "y": "cd"}},
    ]
    assert _sqlaero._encode_value(source).to_bytes() == b"((7,(1,2),(3,'ab')),(8,(4,5),(6,'cd')))"
    assert json.loads(json_dumps(source[::-1]))[0]["id"] == 8


def test_record_fixed_shape_nested_records_and_empty_subarray_axes() -> None:
    source = np.zeros(2, dtype=[("items", [("x", "i8")], (2,)), ("empty", "i8", (0, 3))])
    source["items"]["x"] = [[1, 2], [3, 4]]
    assert json.loads(json_dumps(source)) == [
        {"items": [{"x": 1}, {"x": 2}], "empty": []},
        {"items": [{"x": 3}, {"x": 4}], "empty": []},
    ]


def test_zero_records_and_zero_fields_are_different() -> None:
    assert json_dumps(np.empty(0, dtype=[])) == b"[]"
    assert json_dumps(np.empty(2, dtype=[])) == b"[{},{}]"
    assert json_dumps(np.empty((1, 2), dtype=[])) == b"[[{},{}]]"


def test_record_axes_follow_negative_strides_and_each_field_byte_order() -> None:
    source = np.array([(1, 2), (3, 4), (5, 6), (7, 8)], dtype=[("a", ">i8"), ("b", "<i8")]).reshape(2, 2)
    assert json.loads(json_dumps(source.T[::-1])) == [
        [{"a": 3, "b": 4}, {"a": 7, "b": 8}],
        [{"a": 1, "b": 2}, {"a": 5, "b": 6}],
    ]


def test_record_field_view_never_reads_beyond_the_physical_owner() -> None:
    source = np.array([(7, 8)], dtype=[("a", "i8"), ("b", "i8")])
    invalid = np.lib.stride_tricks.as_strided(source, shape=(2,), strides=(source.itemsize * 2,))
    with pytest.raises(BufferError, match="allocation"):
        json_dumps(invalid)


def test_zero_row_record_does_not_validate_unwritten_json_names() -> None:
    source = np.empty(0, dtype=[("\ud800", "i8")])
    assert json_dumps(source) == b"[]"
    with pytest.raises(UnicodeEncodeError):
        json_dumps(np.zeros(1, dtype=source.dtype))
