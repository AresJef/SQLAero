"""Named records keep real tuple slots and original base-field metadata."""

# pyright: reportPrivateUsage=false


from collections import namedtuple
from typing import NamedTuple

import pytest

from sqlaero import _sqlaero, json_dumps


@pytest.mark.parametrize(("value", "expected"), [
    (namedtuple("Point", "x y")(7, 8), b'{"x":7,"y":8}'),  # pyright: ignore[reportUntypedNamedTuple]  # Exercise collections' real factory.
    (namedtuple("Box", "payload")({"x": 7, "y": 8}), b'{"payload":{"x":7,"y":8}}'),  # pyright: ignore[reportUntypedNamedTuple]  # Exercise collections' real factory.
    (namedtuple("Empty", "")(), b"{}"),  # pyright: ignore[reportUntypedNamedTuple]  # Exercise collections' real factory.
    (namedtuple("Renamed", ["bad field"], rename=True)(7), b'{"_0":7}'),  # pyright: ignore[reportUntypedNamedTuple]  # Exercise collections' real factory.
])
def test_named_records_are_field_objects_at_root_and_nested(value: object, expected: bytes) -> None:
    assert json_dumps(value) == expected
    assert json_dumps([value]) == b"[" + expected + b"]"


def test_typing_namedtuple_uses_the_same_record_contract() -> None:
    class Point(NamedTuple):
        x: int
        y: int
    assert json_dumps(Point(7, 8)) == b'{"x":7,"y":8}'


def test_named_record_subclasses_ignore_field_iteration_and_attribute_replacements() -> None:
    Point = namedtuple("Point", "x y")  # pyright: ignore[reportUntypedNamedTuple]  # Exercise collections' real factory.
    def forbidden(*args: object):
        raise AssertionError("replacement namedtuple access must not execute")
    Child = type("Child", (Point,), {
        "x": property(forbidden), "__getitem__": forbidden, "__iter__": forbidden,
        "__len__": forbidden, "_asdict": forbidden, "_fields": property(forbidden),
    })
    source = Child(7, 8)
    assert json_dumps(source) == b'{"x":7,"y":8}'
    assert _sqlaero._bind_sql("SELECT %s,%s", source) == [b"\x03SELECT 7,8"]
    assert _sqlaero._bind_sql("SELECT %s", source, many=True) == [b"\x03SELECT 7", b"\x03SELECT 8"]


def test_tuple_storage_alone_does_not_make_arbitrary_attributes_a_record_schema() -> None:
    class Plain(tuple[object, ...]):
        _fields = ("name",)
    assert json_dumps(Plain((7,))) == b"[7]"


def test_record_field_values_use_the_same_bounded_error_coordinates() -> None:
    source = namedtuple("Box", "payload")({"amount": float("nan")})  # pyright: ignore[reportUntypedNamedTuple]  # Exercise collections' real factory.
    with pytest.raises(ValueError) as caught:
        json_dumps(source, allow_nan=False)
    assert "data.payload.amount" in "\n".join(caught.value.__notes__)


@pytest.mark.parametrize("fields", [("x",), ["x", "y"], ("x", True)])
def test_invalid_named_record_metadata_does_not_fall_back_to_a_value_array(fields: tuple[object, ...] | list[object]) -> None:
    Point = namedtuple("Point", "x y")  # pyright: ignore[reportUntypedNamedTuple]  # Exercise collections' real factory.
    setattr(Point, "_fields", fields)
    with pytest.raises(TypeError, match="field"):
        json_dumps(Point(7, 8))
    assert _sqlaero._bind_sql("SELECT %s,%s", Point(7, 8)) == [b"\x03SELECT 7,8"]


def test_duplicate_record_names_share_the_object_collision_rule() -> None:
    Point = namedtuple("Point", "x y")  # pyright: ignore[reportUntypedNamedTuple]  # Exercise collections' real factory.
    Point._fields = ("x", "x")
    with pytest.raises(ValueError, match="collid"):
        json_dumps(Point(7, 8))
