"""Enum transparency preserves the current value position and real base value."""

# pyright: reportPrivateUsage=false


from enum import Enum, Flag, IntEnum, IntFlag, StrEnum
import json
import os
from pathlib import Path
import subprocess
import sys
import textwrap

import pytest

from sqlaero import JSON, Cursor, DictCursor, SSCursor, SSDictCursor, _sqlaero, json_dumps
from test_public_connection import live, requires_mysql


class Values(Enum):
    NUMBER = 7
    TEXT = "ab"
    PAIR = (7, 8)
    MISSING = None
    NONFINITE = float("inf")
    BINARY = b"ab"
    UNKNOWN = object()


@pytest.mark.parametrize(("value", "sql", "encoded"), [
    (Values.NUMBER, b"7", b"7"),
    (Values.TEXT, b"'ab'", b'"ab"'),
    (Values.PAIR, b"(7,8)", b"[7,8]"),
    (Values.MISSING, b"NULL", b"null"),
    (Values.NONFINITE, b"NULL", b"null"),
])
def test_enum_member_values_are_consumed_by_each_target(value: Enum, sql: bytes, encoded: bytes) -> None:
    assert _sqlaero._encode_value(value).to_bytes() == sql
    assert json_dumps(value) == encoded
    assert json_dumps([value]) == b"[" + encoded + b"]"


def test_enum_member_values_do_not_replan_the_root_or_many_rows() -> None:
    for many in (False, True):
        for itemize in (False, True):
            assert _sqlaero._escape_projection(Values.PAIR, many=many, itemize=itemize) == "(7,8)"
    assert _sqlaero._bind_sql("SELECT %s", [Values.PAIR], many=True) == [b"\x03SELECT (7,8)"]
    with pytest.raises(Exception, match="scalar|fragment"):
        _sqlaero._encode_native_rows(Values.PAIR)


def test_enum_real_member_storage_ignores_value_and_getattribute_replacements() -> None:
    class Base(Enum):
        @property
        def value(self) -> object:
            raise AssertionError("must not read replaced value")
        def __str__(self) -> str:
            raise AssertionError("must not read display")
    class Child(Base):
        ACTUAL = 17
    def forbidden_getattribute(self: Base, name: str) -> object:
        if name in ("_value_", "__dict__"):
            raise AssertionError("must use actual base dictionary")
        return object.__getattribute__(self, name)
    # Enum itself requires _value_ while constructing members. Install the
    # hostile override only after obtaining a valid source instance.
    Base.__getattribute__ = forbidden_getattribute
    assert _sqlaero._encode_value(Child.ACTUAL).to_bytes() == b"17"
    assert json_dumps(Child.ACTUAL) == b"17"


def test_enum_target_specific_rejection_and_key_rules() -> None:
    assert _sqlaero._encode_value(Values.BINARY).to_bytes() == b"CAST(X'6162' AS BINARY)"
    for value in (Values.BINARY, Values.UNKNOWN):
        with pytest.raises(TypeError):
            json_dumps(value)
    with pytest.raises(ValueError):
        json_dumps(Values.NONFINITE, allow_nan=False)
    with pytest.raises(TypeError):
        json_dumps({Values.NUMBER: "ab"})


def test_enum_integer_and_text_bases_keep_priority_over_member_metadata() -> None:
    class Number(IntEnum):
        VALUE = 7
    class Bits(IntFlag):
        ONE = 1
        TWO = 2
    class Text(StrEnum):
        VALUE = "ab"
    # Deliberately changed member metadata must not replace actual int/str bits.
    Number.VALUE._value_ = 99
    Text.VALUE._value_ = "wrong"
    assert json_dumps([Number.VALUE, Bits.ONE | Bits.TWO, Text.VALUE]) == b'[7,3,"ab"]'
    assert json_dumps({Number.VALUE: "ab", Text.VALUE: 7}) == b'{"7":"ab","ab":7}'
    assert _sqlaero._encode_value(Number.VALUE).to_bytes() == b"7"
    assert _sqlaero._encode_value(Text.VALUE).to_bytes() == b"'ab'"


def test_plain_flag_uses_actual_combination_bits_including_zero() -> None:
    class Bits(Flag):
        ONE = 1
        TWO = 2
    assert json_dumps([Bits(0), Bits.ONE | Bits.TWO]) == b"[0,3]"
    assert _sqlaero._encode_value(Bits.ONE | Bits.TWO).to_bytes() == b"3"


def test_long_transparent_chain_does_not_consume_output_depth_or_native_stack() -> None:
    value: object = 7
    owners: list[Enum] = []
    for _ in range(2000):
        member = Enum("Link", {"VALUE": 0}).VALUE
        member._value_ = value
        owners.append(member)
        value = member
    assert json_dumps(value) == b"7"
    assert _sqlaero._encode_value(value).to_bytes() == b"7"
    nested: object = value
    for _ in range(64):
        nested = [nested]
    assert json.loads(json_dumps(nested)) is not None


@pytest.mark.parametrize("through_container", [False, True])
def test_true_enum_cycles_are_rejected_without_unbounded_recursion(through_container: bool) -> None:
    member = Enum("Loop", {"VALUE": 0}).VALUE
    member._value_ = [member] if through_container else member
    with pytest.raises(ValueError, match="cycle"):
        json_dumps(member)
    with pytest.raises(Exception, match="cycle"):
        _sqlaero._encode_value(member)


def test_shared_noncyclic_enum_member_is_not_mistaken_for_a_cycle() -> None:
    assert json_dumps([Values.NUMBER, Values.NUMBER]) == b"[7,7]"


@pytest.mark.parametrize("target", ["mysql", "json"])
def test_long_enum_chain_checks_signals_inside_the_native_call(target: str) -> None:
    script = "target = " + repr(target) + "\n" + textwrap.dedent("""
        from enum import Enum
        from _native_signal import NativeAlarm, native_call_offsets
        from sqlaero import _sqlaero, json_dumps
        Chain = Enum('Chain', {f'V{i}': i for i in range(50000)})
        owners = list(Chain)
        previous = 7
        for member in owners:
            member._value_ = previous
            previous = member
        source = previous
        original = MemoryError('original signal in transparent native traversal')
        count = 0
        def serialize():
            if target == 'json':
                return json_dumps(source)
            return _sqlaero._encode_value(source)
        offsets = native_call_offsets(serialize.__code__)
        def interrupt(number, frame):
            global count
            if frame is None or frame.f_code is not serialize.__code__ or frame.f_lasti not in offsets:
                return
            count += 1
            if count == 3:
                alarm.cancel()
                raise original
        alarm = NativeAlarm(interrupt)
        try:
            alarm.start(0.0001, 0.0001, wait_ready=True)
            try:
                serialize()
            except MemoryError as error:
                assert error is original and count == 3
            else:
                raise AssertionError('transparent native chain had no growing-work checks')
        finally:
            alarm.close()
    """)
    environment = dict(os.environ)
    environment["PYTHONPATH"] = str(Path(__file__).parent) + os.pathsep + environment.get("PYTHONPATH", "")
    result = subprocess.run([sys.executable, "-c", script], env=environment,
                            capture_output=True, text=True, timeout=20)
    assert result.returncode == 0, (result.stdout, result.stderr)


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("kind", [Cursor, DictCursor, SSCursor, SSDictCursor])
@pytest.mark.parametrize("prepared", [False, True])
@pytest.mark.parametrize(("value", "expected"), [
    (Values.NUMBER, 7), (Values.TEXT, "ab"), (Values.MISSING, None),
])
def test_actual_enum_value_and_json_parameter_results(
    label: str, kind: type[Cursor] | type[DictCursor] | type[SSCursor] | type[SSDictCursor],
    prepared: bool, value: Enum, expected: object,
) -> None:
    with live(label) as connection, connection.cursor(kind, prepared=prepared) as current:
        current.decode_json = True
        current.execute("SELECT %s AS value,CAST(%s AS JSON) AS doc", (value, JSON(value)))
        result = {"value": expected, "doc": expected} if kind in (DictCursor, SSDictCursor) else (expected, expected)
        assert current.fetchone() == result
        assert current.fetchone() is None
