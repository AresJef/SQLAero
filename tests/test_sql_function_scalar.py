"""Conditional, network, cryptographic and session functions on real servers."""

from collections.abc import Callable
from hashlib import sha256
from inspect import signature
from ipaddress import IPv6Address
from uuid import UUID

import pytest
from sqlaero import Connection, sqlfunc
from sqlaero.typing import EscapeInput

from test_public_connection import connection as connection, requires_mysql


EXAMPLE_UUID = UUID("934411c6-928d-11ed-a1eb-0242ac120002")

VALUES: tuple[tuple[str, tuple[object, ...], object], ...] = (
    ("COALESCE", (None, 3, 4), 3),
    ("GREATEST", (2, 4, 3), 4),
    ("IF", (True, 2, 3), 2),
    ("IFNULL", (None, 3), 3),
    ("INTERVAL", (22, 0, 10, 20, 30), 3),
    ("ISNULL", (None,), 1),
    ("LEAST", (2, 4, 3), 2),
    ("NULLIF", (1, 1), None),
    ("BIN_TO_UUID", (EXAMPLE_UUID.bytes,), str(EXAMPLE_UUID)),
    ("CHARSET", ("abc",), "utf8mb4"),
    ("COERCIBILITY", ("abc",), 4),
    ("CURRENT_ROLE", (), "NONE"),
    ("DATABASE", (), "sqlaero_test"),
    ("INET_ATON", ("127.0.0.1",), 0x7F000001),
    ("INET_NTOA", (0x7F000001,), "127.0.0.1"),
    ("INET6_ATON", ("::1",), IPv6Address("::1").packed),
    ("INET6_NTOA", (IPv6Address("::1").packed,), "::1"),
    ("IS_IPV4", ("127.0.0.1",), 1),
    ("IS_IPV4_COMPAT", (IPv6Address("::192.0.2.1").packed,), 1),
    ("IS_IPV4_MAPPED", (IPv6Address("::ffff:192.0.2.1").packed,), 1),
    ("IS_IPV6", ("::1",), 1),
    ("IS_UUID", (str(EXAMPLE_UUID),), 1),
    ("SHA2", ("abc", 256), sha256(b"abc").hexdigest()),
    ("UUID_TO_BIN", (str(EXAMPLE_UUID),), EXAMPLE_UUID.bytes),
)

SPECIAL = (
    "AES_DECRYPT", "AES_ENCRYPT", "COLLATION", "COMPRESS", "CONNECTION_ID",
    "CURRENT_USER", "ICU_VERSION", "LAST_INSERT_ID", "RANDOM_BYTES", "ROW_COUNT",
    "UNCOMPRESS", "UNCOMPRESSED_LENGTH", "USER", "UUID", "UUID_SHORT", "VERSION",
)


def test_scalar_function_public_identities() -> None:
    names = {row[0] for row in VALUES} | set(SPECIAL)
    assert len(names) == 40
    for name in names:
        constructor: type[object] = getattr(sqlfunc, name)
        assert name in sqlfunc.__all__
        assert constructor.__name__ == name and constructor.__module__ == "sqlaero.sqlfunc"
        assert constructor.__base__ is sqlfunc.ABS.__base__
        assert constructor.__doc__
        assert signature(constructor).parameters is not None


@requires_mysql
@pytest.mark.parametrize(("name", "args", "expected"), VALUES)
def test_scalar_function_result(
    connection: Connection, name: str, args: tuple[object, ...], expected: object,
) -> None:
    constructor: Callable[..., EscapeInput] = getattr(sqlfunc, name)
    expr = constructor(*args)
    expected_args = ",".join(str(connection.escape(value)) for value in args)  # type: ignore[arg-type]
    assert connection.escape(expr) == f"{name}({expected_args})"
    with connection.cursor() as cursor:
        cursor.execute("SELECT %s", expr)
        assert cursor.fetchone() == (expected,)


@requires_mysql
def test_server_version_and_session_values_match_fixed_sql(connection: Connection) -> None:
    with connection.cursor() as cursor:
        cursor.execute(
            "SELECT COLLATION('x'), CONNECTION_ID(), CURRENT_USER(), ICU_VERSION(), USER(), VERSION()"
        )
        expected = cursor.fetchone()
        assert expected is not None
        cursor.execute(
            "SELECT %s,%s,%s,%s,%s,%s",
            (sqlfunc.COLLATION("x"), sqlfunc.CONNECTION_ID(), sqlfunc.CURRENT_USER(),
             sqlfunc.ICU_VERSION(), sqlfunc.USER(), sqlfunc.VERSION()),
        )
        assert cursor.fetchone() == expected


@requires_mysql
def test_uuid_random_and_binary_representations(connection: Connection) -> None:
    plain = EXAMPLE_UUID.bytes
    swapped = plain[6:8] + plain[4:6] + plain[:4] + plain[8:]
    with connection.cursor() as cursor:
        cursor.execute("SELECT %s,%s,%s,%s,%s", (
            sqlfunc.UUID(), sqlfunc.UUID_SHORT(), sqlfunc.RANDOM_BYTES(4),
            sqlfunc.UUID_TO_BIN(str(EXAMPLE_UUID), 1), sqlfunc.BIN_TO_UUID(swapped, 1),
        ))
        row = cursor.fetchone()
        assert row is not None and isinstance(row[0], str) and UUID(row[0]).version == 1
        assert isinstance(row[1], int) and row[1] > 0
        assert isinstance(row[2], bytes) and len(row[2]) == 4
        assert row[3:] == (swapped, str(EXAMPLE_UUID))


@requires_mysql
def test_encryption_and_compression_execute_at_the_server(connection: Connection) -> None:
    packed = sqlfunc.COMPRESS("abc")
    encrypted = sqlfunc.AES_ENCRYPT("abc", "key")
    with connection.cursor() as cursor:
        cursor.execute(
            "SELECT %s,%s,%s,%s,%s",
            (packed, sqlfunc.UNCOMPRESS(packed), sqlfunc.UNCOMPRESSED_LENGTH(packed),
             encrypted, sqlfunc.AES_DECRYPT(encrypted, "key")),
        )
        row = cursor.fetchone()
        assert row is not None and isinstance(row[0], bytes) and row[0][:4] == b"\x03\0\0\0"
        assert row[1:3] == (b"abc", 3)
        assert isinstance(row[3], bytes) and len(row[3]) == 16 and row[4] == b"abc"
        with pytest.raises(TypeError, match="omitted"):
            sqlfunc.AES_ENCRYPT("abc", "key", kdf="hkdf")
        assert connection.escape(sqlfunc.AES_ENCRYPT(
            "abc", "key", "0123456789abcdef", "hkdf", "salt", "info",
        )) == "AES_ENCRYPT('abc','key','0123456789abcdef','hkdf','salt','info')"


@requires_mysql
def test_session_mutation_happens_only_when_the_function_executes(connection: Connection) -> None:
    with connection.cursor() as cursor:
        cursor.execute("SELECT LAST_INSERT_ID(0)")
        assert cursor.fetchone() == (0,)
        change = sqlfunc.LAST_INSERT_ID(51)
        assert cursor.mogrify("SELECT %s", change) == "SELECT LAST_INSERT_ID(51)"
        cursor.execute("SELECT %s", sqlfunc.LAST_INSERT_ID())
        assert cursor.fetchone() == (0,)
        cursor.execute("SELECT %s", change)
        assert cursor.fetchone() == (51,)
        cursor.execute("SELECT %s", sqlfunc.LAST_INSERT_ID())
        assert cursor.fetchone() == (51,)

        cursor.execute("CREATE TEMPORARY TABLE sqlaero_row_count (n INT)")
        cursor.execute("INSERT INTO sqlaero_row_count VALUES (1),(2)")
        cursor.execute("SELECT %s", sqlfunc.ROW_COUNT())
        assert cursor.fetchone() == (2,)
