"""Approved standard protocols, conversion counts and common cycle identity."""
# pyright: reportPrivateUsage=false
from datetime import date, datetime, time, timedelta
from decimal import Decimal, DecimalTuple
from enum import IntEnum, StrEnum
from collections.abc import Iterator
import time as clock
import pytest
from sqlaero import _sqlaero


def encoded(value: object) -> bytes:
    return _sqlaero._encode_value(value).to_bytes()


class Integer(int):
    def __int__(self) -> int:
        return 41

class Floating(float):
    def __float__(self) -> float:
        return -0.0

class Text(str):
    def __str__(self) -> str:
        return "x'\\y"

class Binary(bytes):
    def __bytes__(self) -> bytes:
        return b"x\x00\xff"

class ExactDecimal(Decimal):
    def as_tuple(self) -> DecimalTuple:
        raise AssertionError("the base Decimal constructor must protect its value")


@pytest.mark.parametrize("value,expected", [
    (Integer(1), b"41"), (Floating(2.5), b"-0.0e0"),
    (Text("ignored"), b"'x\\'\\\\y'"),
    (Binary(b"ignored"), b"CAST(X'7800FF' AS BINARY)"),
    (ExactDecimal("1.2300"), b"1.23"),
])
def test_scalar_subclasses_keep_approved_protocol_results(value: object, expected: bytes) -> None:
    assert encoded(value) == expected


@pytest.mark.parametrize("base,value,expected", [
    (date, (2024, 1, 2), b"'2024-01-02'"),
    (datetime, (2024, 1, 2, 3, 4, 5), b"'2024-01-02 03:04:05'"),
    (time, (3, 4, 5), b"'03:04:05'"),
    (timedelta, (1, 2), b"'24:00:02'"),
])
def test_temporal_subclasses_read_native_fields(base: type, value: tuple[int, ...], expected: bytes) -> None:
    def forbidden(_self: object) -> str:
        raise AssertionError("no temporal string conversion")
    Child = type("Child", (base,), {"__str__": forbidden})
    assert encoded(Child(*value)) == expected


@pytest.mark.parametrize("base", [list, tuple])
@pytest.mark.parametrize("itemize", [False, True])
def test_sequence_subclass_normalizes_once_and_binds_the_formal_values(base: type, itemize: bool) -> None:
    seen: list[str] = []
    def iterate(_self: object) -> Iterator[int]:
        seen.append("iterate")
        return iter((11, 22))
    Child = type("Child", (base,), {"__iter__": iterate})
    source = Child([1, 2])
    sql = "SELECT %s,%s" if itemize else "SELECT %s"
    expected = b"\x03SELECT 11,22" if itemize else b"\x03SELECT (11,22)"
    assert _sqlaero._bind_sql(sql, source, itemize=itemize) == [expected]
    assert seen == ["iterate"]


def test_normalization_retains_the_original_cycle_identity() -> None:
    seen: list[str] = []
    class Child(list[object]):
        def __iter__(self) -> Iterator[object]:
            seen.append("iterate")
            return super().__iter__()
    value = Child()
    value.append(value)
    with pytest.raises(Exception, match="true cycle"):
        encoded(value)
    assert seen == ["iterate"]


@pytest.mark.parametrize("child_levels", [63, 64])
def test_normalized_container_alias_does_not_add_a_logical_level(child_levels: int) -> None:
    class Child(list[object]):
        pass

    value: object = 7
    for _ in range(child_levels):
        value = [value]
    source = Child([value])
    if child_levels == 63:
        assert encoded(source) == b"(" * 64 + b"7" + b")" * 64
    else:
        with pytest.raises(_sqlaero.DataError) as caught:
            encoded(source)
        assert caught.value.code == 65543


def test_formal_conversion_original_exception_is_not_replaced() -> None:
    original = KeyboardInterrupt("actual conversion error")
    class Child(int):
        def __int__(self) -> int:
            raise original
    with pytest.raises(KeyboardInterrupt) as caught:
        encoded([Child(1)])
    assert caught.value is original


def test_dict_subclass_uses_the_formal_base_constructor() -> None:
    class Child(dict[str, object]):
        def values(self):
            raise AssertionError("base conversion precedes values semantics")
    assert encoded(Child(a=1, b=2)) == b"(1,2)"


@pytest.mark.parametrize("base", [set, frozenset])
def test_set_subclasses_are_real_families_and_keep_actual_base_order(base: type) -> None:
    Child = type("Child", (base,), {})
    source = Child([31, 3, 97])
    assert encoded(source) == b"(" + b",".join(str(value).encode() for value in base(source)) + b")"


def test_enums_follow_their_real_base_families() -> None:
    class Number(IntEnum):
        VALUE = 7
    class Name(StrEnum):
        VALUE = "answer"
    assert encoded(Number.VALUE) == b"7"
    assert encoded(Name.VALUE) == b"'answer'"


@pytest.mark.parametrize("parts,expected", [
    ((2024, 1, 2, 3, 4, 5), b"'2024-01-02 03:04:05'"),
    ((2024, 13, 31, 25, 59, 61), b"'2024-13-31 25:59:61'"),
    ((0, 0, 0, 0, 0, 0), b"'0000-00-00 00:00:00'"),
])
def test_struct_time_encodes_six_components_without_target_calendar_precheck(parts: tuple[int, ...], expected: bytes) -> None:
    value = clock.struct_time((*parts, 999, 999, 999))
    assert encoded(value) == expected
    assert _sqlaero._bind_sql("SELECT %s", value) == [b"\x03SELECT " + expected]


@pytest.mark.parametrize("index", range(6))
def test_struct_time_rejects_negative_components(index: int) -> None:
    parts = [2024, 1, 2, 3, 4, 5]
    parts[index] = -1
    with pytest.raises(ValueError, match="nonnegative"):
        encoded(clock.struct_time((*parts, 0, 0, 0)))


@pytest.mark.parametrize("bad", [1.5, "1", Decimal("1")])
def test_struct_time_refuses_int_truncation_or_numeric_text(bad: object) -> None:
    with pytest.raises(TypeError):
        encoded(clock.struct_time((2024, bad, 1, 0, 0, 0, 0, 0, 0)))


def test_struct_time_accepts_integer_protocol_exactly_once() -> None:
    seen: list[str] = []
    class Index:
        def __index__(self) -> int:
            seen.append("index")
            return 2
    assert encoded(clock.struct_time((2024, Index(), 1, 0, 0, 0, 0, 0, 0))) == b"'2024-02-01 00:00:00'"
    assert seen == ["index"]


def test_bytearray_subclasses_do_not_reenter_through_bytes_protocol() -> None:
    class Excluded(bytearray):
        def __bytes__(self) -> bytes:
            return b"allowed-looking"
    with pytest.raises(TypeError):
        encoded(Excluded(b"x"))


@pytest.mark.parametrize('many', [False, True])
def test_preparation_failures_keep_original_exception_and_root_or_row_location(many: bool) -> None:
    original = MemoryError('actual preparation failure')
    class Broken(list[object]):
        def __iter__(self) -> Iterator[object]:
            raise original
    source = [(1,), Broken([2])] if many else Broken([2])
    with pytest.raises(MemoryError) as caught:
        _sqlaero._bind_sql('SELECT %s', source, many=many)
    assert caught.value is original
    location = 'args[1]' if many else 'args'
    assert any(f'while binding {location};' in note for note in getattr(original, '__notes__', ()))
