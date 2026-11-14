"""UUID uses base slot bits, not display methods, in both targets and keys."""

# pyright: reportPrivateUsage=false


from uuid import UUID
import re

import pytest

from sqlaero import _sqlaero, json_dumps


@pytest.mark.parametrize(("value", "text"), [
    (UUID("12345678-1234-5678-1234-567812345678"), "12345678-1234-5678-1234-567812345678"),
    (UUID(int=0), "00000000-0000-0000-0000-000000000000"),
    (UUID(int=2**128-1), "ffffffff-ffff-ffff-ffff-ffffffffffff"),
])
def test_uuid_canonical_text_for_sql_json_and_keys(value: UUID, text: str) -> None:
    assert _sqlaero._encode_value(value).to_bytes() == b"'" + text.encode() + b"'"
    assert json_dumps(value) == b'"' + text.encode() + b'"'
    assert json_dumps({value: 7}) == b'{"' + text.encode() + b'":7}'
    with pytest.raises(ValueError, match="collid"):
        json_dumps({value: 7, text: 8})
    assert _sqlaero._escape_projection({value:7})==("7",)
    assert _sqlaero._escape_projection({value:7,text:8})==("7","8")
    with pytest.raises(TypeError):json_dumps(value.bytes)
    binary=_sqlaero._encode_value(value.bytes).to_bytes()
    match=re.fullmatch(rb"CAST\(X'([0-9a-fA-F]*)' AS BINARY\)",binary)
    assert match is not None
    assert bytes.fromhex(match.group(1).decode())==value.bytes


def test_true_uuid_subclass_ignores_replaced_int_and_display() -> None:
    def forbidden_int(self: object) -> int:
        raise AssertionError("read canonical UUID.int slot")
    def forbidden_string(self: object) -> str:
        raise AssertionError("no UUID display method")
    Child = type("Child", (UUID,), {"int": property(forbidden_int), "__str__": forbidden_string})
    # Base construction stores the canonical slot through object.__setattr__.
    value = object.__new__(Child)
    vars(UUID)["int"].__set__(value, 7)
    vars(UUID)["is_safe"].__set__(value, None)
    assert json_dumps(value) == b'"00000000-0000-0000-0000-000000000007"'
    assert _sqlaero._encode_value(value).to_bytes() == b"'00000000-0000-0000-0000-000000000007'"


def test_uuid_key_reads_base_bits_without_calling_the_replaced_hash_or_int() -> None:
    class Integer(int):
        def __int__(self):
            raise AssertionError("no int override")
        def __index__(self):
            raise AssertionError("no index override")
    value = UUID(int=7)
    vars(UUID)["int"].__set__(value, Integer(7))
    assert json_dumps(value) == b'"00000000-0000-0000-0000-000000000007"'


@pytest.mark.parametrize("bad", [-1, 1 << 128, 1 << 2_000_000, "7"], ids=["negative", "129-bits", "very-large", "not-an-int"])
def test_corrupted_uuid_slot_is_rejected_before_unbounded_or_custom_conversion(bad: object) -> None:
    value = UUID(int=0)
    vars(UUID)["int"].__set__(value, bad)
    with pytest.raises((TypeError, ValueError), match="UUID.*actual"):
        json_dumps(value)
