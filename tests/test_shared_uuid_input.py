"""UUID uses base slot bits, not display methods, in both targets and keys."""

from uuid import UUID

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


def test_true_uuid_subclass_ignores_replaced_int_and_display() -> None:
    class Child(UUID):
        @property
        def int(self):
            raise AssertionError("read canonical UUID.int slot")
        def __str__(self):
            raise AssertionError("no UUID display method")
    # Base construction stores the canonical slot through object.__setattr__.
    value = object.__new__(Child)
    UUID.int.__set__(value, 7)
    UUID.is_safe.__set__(value, None)
    assert json_dumps(value) == b'"00000000-0000-0000-0000-000000000007"'
    assert _sqlaero._encode_value(value).to_bytes() == b"'00000000-0000-0000-0000-000000000007'"


def test_uuid_key_reads_base_bits_without_calling_the_replaced_hash_or_int() -> None:
    class Integer(int):
        def __int__(self):
            raise AssertionError("no int override")
        def __index__(self):
            raise AssertionError("no index override")
    value = UUID(int=7)
    UUID.int.__set__(value, Integer(7))
    assert json_dumps(value) == b'"00000000-0000-0000-0000-000000000007"'


@pytest.mark.parametrize("bad", [-1, 1 << 128, 1 << 2_000_000, "7"], ids=["negative", "129-bits", "very-large", "not-an-int"])
def test_corrupted_uuid_slot_is_rejected_before_unbounded_or_custom_conversion(bad: object) -> None:
    value = UUID(int=0)
    UUID.int.__set__(value, bad)
    with pytest.raises((TypeError, ValueError), match="UUID.*actual"):
        json_dumps(value)
