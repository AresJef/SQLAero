"""Explicit BMP-only UTF8MB3 remains separate from UTF-8/UTF8MB4 aliases."""

import pytest
from sqlaero import _sqlaero
from test_public_connection import live, requires_mysql


@pytest.mark.parametrize("strict", [False, True])
def test_utf8mb3_result_metadata_rejects_four_byte_sequences_or_preserves_bytes(strict: bool) -> None:
    data = "中😀".encode()
    decode = getattr(_sqlaero, "_decode_field")
    if strict:
        with pytest.raises(UnicodeDecodeError) as failure:
            decode(253, data, collation_id=33, decode_text_strict=True)
        assert failure.value.object == data and failure.value.encoding == "utf8mb3"
        assert (failure.value.start, failure.value.end) == (3, 4)
    else:
        assert decode(253, data, collation_id=33, decode_text_strict=False) == "中\udcf0\udc9f\udc98\udc80"


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
def test_explicit_utf8mb3_connects_and_keeps_text_and_binary_field_roles(label: str) -> None:
    with live(label, charset="utf8mb3") as connection, connection.cursor() as cursor:
        cursor.execute("SELECT @@character_set_client,@@collation_connection")
        assert cursor.fetchone() == ("utf8mb3", "utf8mb3_general_ci")
        text = "中文\uffff"
        binary = "😀".encode()
        cursor.execute("SELECT %s,HEX(%s),%s", (text, text, binary))
        assert cursor.fetchone() == (text, text.encode().hex().upper(), binary)
        cursor.execute("SELECT 123 AS v UNION ALL SELECT 456")
        assert cursor.fetchone() == (123,)
        columns = cursor.columns
        source = "中😀"
        with pytest.raises(UnicodeEncodeError) as failure:
            cursor.execute("SELECT %s", source)
        assert failure.value.object is source
        assert (failure.value.encoding, failure.value.start, failure.value.end) == ("utf8mb3", 1, 2)
        assert cursor.columns is columns and cursor.fetchone() == (456,)


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("alias", ["utf8", "utf-8"])
def test_historical_utf8_aliases_still_select_mb4_and_accept_supplementary_text(label: str, alias: str) -> None:
    with live(label, charset=alias) as connection, connection.cursor() as cursor:
        cursor.execute("SELECT @@character_set_client,%s", "😀")
        assert cursor.fetchone() == ("utf8mb4", "😀")
