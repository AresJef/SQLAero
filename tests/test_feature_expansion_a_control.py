"""Thin control inputs stay native and preserve their specified protocols."""

# pyright: reportPrivateUsage=false

from typing import Any, Never
import re

import pytest
from sqlaero import _sqlaero


def request(name: object, **options: Any) -> bytes:
    return getattr(_sqlaero, "_savepoint_request")(name, **options)


def initialize(value: object = None) -> list[str]:
    return getattr(_sqlaero, "_prepare_init_sql")(value)


@pytest.mark.parametrize(("action", "prefix"), [("create", b"SAVEPOINT "),
                                                ("rollback", b"ROLLBACK TO SAVEPOINT "),
                                                ("release", b"RELEASE SAVEPOINT ")])
def test_savepoint_quotes_one_original_identifier(action: str, prefix: bytes) -> None:
    assert request(" x`'; SELECT 1 \\ ", action=action) == b"\x03" + prefix + b"` x``'; SELECT 1 \\ `"
    assert request("表`〜", action=action, charset="sjis") == b"\x03" + prefix + b"`\x95\x5c``\x81\x60`"


class NameString(str):
    def __str__(self) -> str:
        raise AssertionError("savepoint names use base string contents")

    def __len__(self) -> int:
        raise AssertionError("savepoint names use base string length")


def test_savepoint_name_subclass_uses_base_contents_without_escape_protocols() -> None:
    assert request(NameString("real`name")) == b"\x03SAVEPOINT `real``name`"


@pytest.mark.parametrize("name", [None, 1, b"bytes", object()])
def test_savepoint_wrong_types_are_local_type_errors(name: object) -> None:
    with pytest.raises(TypeError, match="name"):
        request(name)


@pytest.mark.parametrize("name", ["", "a\0b"])
def test_savepoint_empty_and_nul_names_are_local_value_errors(name: str) -> None:
    with pytest.raises(ValueError, match="name"):
        request(name)


def test_savepoint_encoding_and_whole_request_limits_preserve_positions() -> None:
    text = "中😀"
    with pytest.raises(UnicodeEncodeError) as failure:
        request(text, charset="utf8mb3")
    assert failure.value.object is text
    assert (failure.value.encoding, failure.value.start, failure.value.end) == ("utf8mb3", 1, 2)
    expected = b"\x03SAVEPOINT `a``b`"
    assert request("a`b", limit=len(expected)) == expected
    from sqlaero.errors import OperationalError
    with pytest.raises(OperationalError):
        request("a`b", limit=len(expected) - 1)


def test_init_sql_is_finite_ordered_rust_owned_text_without_semicolon_splitting() -> None:
    assert initialize() == []
    assert initialize([]) == []
    assert initialize(()) == []
    assert initialize("SELECT 1; SELECT 2") == ["SELECT 1; SELECT 2"]
    assert initialize(("SET @x=1", "SELECT @x")) == ["SET @x=1", "SELECT @x"]
    assert initialize("SELECT '\x00'") == ["SELECT '\x00'"]
    assert initialize(NameString("SELECT 'base'")) == ["SELECT 'base'"]


class SQLList(list[str]):
    def __iter__(self) -> Any:
        raise AssertionError("init_sql reads base finite sequence")

    def __getitem__(self, _index: Any) -> Never:
        raise AssertionError("init_sql reads base finite sequence")


def test_init_sql_reads_base_finite_sequence_without_user_iteration() -> None:
    assert initialize(SQLList(["SELECT 1", "SELECT 2"])) == ["SELECT 1", "SELECT 2"]


@pytest.mark.parametrize("value", [b"SELECT 1", iter(["SELECT 1"]), 1, object()])
def test_init_sql_rejects_unsupported_carriers_without_iterating(value: object) -> None:
    with pytest.raises(TypeError, match="init_sql"):
        initialize(value)


@pytest.mark.parametrize(("value", "kind", "label"), [("", ValueError, "init_sql"),
    (["SELECT 1", ""], ValueError, "init_sql[1]"),
    (["SELECT 1", None], TypeError, "init_sql[1]"), ([b"SELECT 1"], TypeError, "init_sql[0]")])
def test_init_sql_member_errors_identify_the_original_step(value: object, kind: type[Exception], label: str) -> None:
    with pytest.raises(kind, match=re.escape(label)):
        initialize(value)


def test_control_long_values_keep_original_unicode_positions_and_work_checks() -> None:
    text = "中" * 100_000 + "😀"
    with pytest.raises(UnicodeEncodeError) as failure:
        request(text, charset="utf8mb3")
    assert failure.value.object is text and (failure.value.start, failure.value.end) == (100_000, 100_001)
    from sqlaero.errors import OperationalTimeoutError
    with pytest.raises(OperationalTimeoutError):
        request("x" * 300_000, timeout_ns=1)
    sql = "SELECT '" + "中" * 100_000 + "'"
    assert initialize([sql, "SELECT 2"]) == [sql, "SELECT 2"]


def test_native_rust_sql_source_encoding_adds_no_quotes_or_command_and_preserves_prefix() -> None:
    encode = getattr(_sqlaero, "_sql_utf8")
    assert encode("SELECT '中' \\ %s %%") == "SELECT '中' \\ %s %%".encode()
    assert encode("é'\\", charset="latin1", prefix=b"\x03") == b"\x03\xe9'\\"
    assert encode("\\", charset="sjis") == b"\x81\x5f"
    assert encode("", prefix=b"kept") == b"kept"


def test_native_rust_sql_source_encoding_checks_complete_size_work_and_global_error_offset() -> None:
    encode = getattr(_sqlaero, "_sql_utf8")
    source = "中" * 100_000 + "😀"
    with pytest.raises(UnicodeEncodeError) as failure:
        encode(source, charset="utf8mb3")
    assert failure.value.object == source and type(failure.value.object) is str
    assert (failure.value.encoding, failure.value.start, failure.value.end) == ("utf8mb3", 100_000, 100_001)
    assert encode("abc", prefix=b"\x03", limit=4) == b"\x03abc"
    from sqlaero.errors import OperationalError, OperationalTimeoutError
    with pytest.raises(OperationalError):
        encode("abc", prefix=b"\x03", limit=3)
    with pytest.raises(OperationalTimeoutError):
        encode("x" * 300_000, timeout_ns=1)
