"""The string and regular expression catalog through the public entry points."""

from collections.abc import Callable
from inspect import signature

import pytest
from sqlaero import Connection, sqlfunc
from sqlaero.typing import EscapeInput

from test_public_connection import connection as connection, requires_mysql


STRINGS: tuple[tuple[str, tuple[object, ...], str, object], ...] = (
    ("ASCII", ("A",), "ASCII('A')", 65),
    ("BIN", (5,), "BIN(5)", "101"),
    ("BIT_LENGTH", ("é",), "BIT_LENGTH('é')", 16),
    ("CHAR", (65,), "CHAR(65)", b"A"),
    ("CHAR_LENGTH", ("é",), "CHAR_LENGTH('é')", 1),
    ("CONCAT", ("a", "b", "c"), "CONCAT('a','b','c')", "abc"),
    ("CONCAT_WS", (",", None, "b"), "CONCAT_WS(',',NULL,'b')", "b"),
    ("ELT", (2, "a", "b"), "ELT(2,'a','b')", "b"),
    ("EXPORT_SET", (5, "Y", "N", ":", 3), "EXPORT_SET(5,'Y','N',':',3)", "Y:N:Y"),
    ("FIELD", ("b", "a", "b"), "FIELD('b','a','b')", 2),
    ("FIND_IN_SET", ("b", "a,b"), "FIND_IN_SET('b','a,b')", 2),
    ("FORMAT", (1234, 2, "en_US"), "FORMAT(1234,2,'en_US')", "1,234.00"),
    ("FROM_BASE64", ("QQ==",), "FROM_BASE64('QQ==')", b"A"),
    ("HEX", (255,), "HEX(255)", "FF"),
    ("INSERT", ("abc", 2, 1, "Z"), "INSERT('abc',2,1,'Z')", "aZc"),
    ("INSTR", ("abc", "b"), "INSTR('abc','b')", 2),
    ("LEFT", ("abc", 2), "LEFT('abc',2)", "ab"),
    ("LENGTH", ("é",), "LENGTH('é')", 2),
    ("LOCATE", ("b", "abcb", 3), "LOCATE('b','abcb',3)", 4),
    ("LOWER", ("ABC",), "LOWER('ABC')", "abc"),
    ("LPAD", ("a", 3, "0"), "LPAD('a',3,'0')", "00a"),
    ("LTRIM", ("  a ",), "LTRIM('  a ')", "a "),
    ("MAKE_SET", (5, "a", "b", "c"), "MAKE_SET(5,'a','b','c')", "a,c"),
    ("OCT", (8,), "OCT(8)", "10"),
    ("ORD", ("A",), "ORD('A')", 65),
    ("POSITION", ("b", "abc"), "POSITION('b' IN 'abc')", 2),
    ("QUOTE", ("abc",), "QUOTE('abc')", "'abc'"),
    ("REGEXP_INSTR", ("aa bb", "b", 1, 2, 0, "c"), "REGEXP_INSTR('aa bb','b',1,2,0,'c')", 5),
    ("REGEXP_LIKE", ("ABC", "a", "i"), "REGEXP_LIKE('ABC','a','i')", 1),
    ("REGEXP_REPLACE", ("a0a", "a", "b", 1, 2, "c"), "REGEXP_REPLACE('a0a','a','b',1,2,'c')", "a0b"),
    ("REGEXP_SUBSTR", ("aa baa", "a+", 1, 2, "c"), "REGEXP_SUBSTR('aa baa','a+',1,2,'c')", "aa"),
    ("REPEAT", ("A", 3), "REPEAT('A',3)", "AAA"),
    ("REPLACE", ("abc", "b", "Z"), "REPLACE('abc','b','Z')", "aZc"),
    ("REVERSE", ("abc",), "REVERSE('abc')", "cba"),
    ("RIGHT", ("abc", 2), "RIGHT('abc',2)", "bc"),
    ("RPAD", ("a", 3, "0"), "RPAD('a',3,'0')", "a00"),
    ("RTRIM", (" a  ",), "RTRIM(' a  ')", " a"),
    ("SOUNDEX", ("Hello",), "SOUNDEX('Hello')", "H400"),
    ("SPACE", (3,), "SPACE(3)", "   "),
    ("STRCMP", ("a", "b"), "STRCMP('a','b')", -1),
    ("SUBSTRING", ("abc", 2, 1), "SUBSTRING('abc',2,1)", "b"),
    ("SUBSTRING_INDEX", ("a:b:c", ":", -1), "SUBSTRING_INDEX('a:b:c',':',-1)", "c"),
    ("TO_BASE64", ("A",), "TO_BASE64('A')", "QQ=="),
    ("TRIM", ("  abc  ",), "TRIM('  abc  ')", "abc"),
    ("UNHEX", ("41",), "UNHEX('41')", b"A"),
    ("UPPER", ("abc",), "UPPER('abc')", "ABC"),
)


def test_string_constructors_are_named_final_public_types() -> None:
    assert len({row[0] for row in STRINGS}) == 46
    for name, args, _, _ in STRINGS:
        constructor: type[object] = getattr(sqlfunc, name)
        assert name in sqlfunc.__all__
        assert constructor.__name__ == name and constructor.__module__ == "sqlaero.sqlfunc"
        assert constructor.__base__ is sqlfunc.ABS.__base__
        assert constructor.__doc__
        signature(constructor).bind(*args)
        with pytest.raises(TypeError):
            constructor()  # type: ignore[call-arg]


@requires_mysql
@pytest.mark.parametrize(("name", "args", "text", "expected"), STRINGS)
def test_string_function_results(
    connection: Connection, name: str, args: tuple[object, ...], text: str, expected: object,
) -> None:
    constructor: Callable[..., EscapeInput] = getattr(sqlfunc, name)
    expression = constructor(*args)
    assert connection.escape(expression) == text
    with connection.cursor() as cursor:
        cursor.execute("SELECT %s", expression)
        assert cursor.fetchone() == (expected,)


@requires_mysql
def test_controlled_charset_and_trim_roles(connection: Connection) -> None:
    with pytest.raises(ValueError, match="charset"):
        sqlfunc.CHAR(65, using="utf8mb4) ; SELECT 9")
    with pytest.raises(ValueError, match="direction"):
        sqlfunc.TRIM("abc", direction="left")
    with pytest.raises(TypeError, match="omitted"):
        sqlfunc.REGEXP_REPLACE("abc", "a", "b", occurrence=2)
    char = sqlfunc.CHAR(65, 66, using="UTF8MB4")
    leading = sqlfunc.TRIM("''x''", remove="'", direction="leading")
    trailing = sqlfunc.TRIM("''x''", remove="'", direction="trailing")
    assert connection.escape(char) == "CHAR(65,66 USING utf8mb4)"
    assert connection.escape(leading) == "TRIM(LEADING '\\'' FROM '\\'\\'x\\'\\'')"
    with connection.cursor() as cursor:
        cursor.execute("SELECT %s,%s,%s", (char, leading, trailing))
        assert cursor.fetchone() == ("AB", "x''", "''x")
