"""Public numeric SQL functions, checked individually on both MySQL versions."""

from collections.abc import Callable
from inspect import Parameter, signature
import math
import zlib

import pytest
from sqlaero import Connection, sqlfunc
from sqlaero.typing import EscapeInput

from test_public_connection import connection as connection, requires_mysql


NUMERIC: tuple[tuple[str, tuple[object, ...], str, object, tuple[str, ...]], ...] = (
    ("ABS", (-7,), "-7", 7, ("value",)),
    ("ACOS", (0,), "0", math.pi / 2, ("value",)),
    ("ASIN", (1,), "1", math.pi / 2, ("value",)),
    ("ATAN", (1,), "1", math.pi / 4, ("y", "x")),
    ("ATAN", (1, 0), "1,0", math.pi / 2, ("y", "x")),
    ("ATAN2", (1, 0), "1,0", math.pi / 2, ("y", "x")),
    ("BIT_COUNT", (7,), "7", 3, ("value",)),
    ("CEIL", (-7,), "-7", -7, ("value",)),
    ("CONV", ("ff", 16, 10), "'ff',16,10", "255", ("value", "from_base", "to_base")),
    ("COS", (0,), "0", 1, ("value",)),
    ("COT", (1,), "1", 1 / math.tan(1), ("value",)),
    ("CRC32", ("a",), "'a'", zlib.crc32(b"a"), ("value",)),
    ("DEGREES", (1,), "1", 180 / math.pi, ("radians",)),
    ("EXP", (0,), "0", 1, ("value",)),
    ("FLOOR", (-7,), "-7", -7, ("value",)),
    ("LN", (1,), "1", 0, ("value",)),
    ("LOG", (8,), "8", math.log(8), ("base_or_value", "value")),
    ("LOG", (2, 8), "2,8", 3, ("base_or_value", "value")),
    ("LOG10", (100,), "100", 2, ("value",)),
    ("LOG2", (8,), "8", 3, ("value",)),
    ("MOD", (-7, 2), "-7,2", -1, ("dividend", "divisor")),
    ("PI", (), "", 3.141593, ()),
    ("POW", (2, 3), "2,3", 8, ("base", "exponent")),
    ("RADIANS", (180,), "180", math.pi, ("degrees",)),
    ("RAND", (), "", None, ("seed",)),
    ("RAND", (13,), "13", None, ("seed",)),
    ("ROUND", (4,), "4", 4, ("value", "digits")),
    ("ROUND", (15, -1), "15,-1", 20, ("value", "digits")),
    ("SIGN", (-7,), "-7", -1, ("value",)),
    ("SIN", (0,), "0", 0, ("value",)),
    ("SQRT", (4,), "4", 2, ("value",)),
    ("TAN", (0,), "0", 0, ("value",)),
    ("TRUNCATE", (29, -1), "29,-1", 20, ("value", "digits")),
)


def test_numeric_constructor_signatures_and_exports() -> None:
    assert len({row[0] for row in NUMERIC}) == 29
    for name, args, _, _, parameters in NUMERIC:
        constructor: type[object] = getattr(sqlfunc, name)
        assert name in sqlfunc.__all__
        assert constructor.__name__ == name and constructor.__module__ == "sqlaero.sqlfunc"
        assert constructor.__base__ is sqlfunc.ABS.__base__
        assert constructor.__doc__
        public_signature = signature(constructor)
        assert tuple(public_signature.parameters) == parameters
        public_signature.bind(*args)
        with pytest.raises(TypeError):
            public_signature.bind(*(*args, 1, 2, 3))
        required = sum(p.default is Parameter.empty for p in public_signature.parameters.values())
        for arity in (required - 1, len(parameters) + 1):
            if arity >= 0:
                with pytest.raises(TypeError):
                    constructor(*([1] * arity))  # type: ignore[call-arg]


@requires_mysql
@pytest.mark.parametrize(("name", "args", "arguments", "expected", "_parameters"), NUMERIC)
def test_numeric_functions_render_and_execute(
    connection: Connection, name: str, args: tuple[object, ...], arguments: str,
    expected: object, _parameters: tuple[str, ...],
) -> None:
    constructor: Callable[..., EscapeInput] = getattr(sqlfunc, name)
    expression = constructor(*args)
    assert connection.escape(expression) == f"{name}({arguments})"
    with connection.cursor() as cursor:
        cursor.execute("SELECT %s", expression)
        row = cursor.fetchone()
        assert row is not None
        result = row[0]
        if name == "RAND":
            assert isinstance(result, float) and 0 <= result < 1
            if args:
                cursor.execute("SELECT RAND(13)")
                assert cursor.fetchone() == row
        elif isinstance(expected, float):
            assert isinstance(result, (float, int)) and math.isclose(result, expected, rel_tol=1e-12)
        else:
            assert result == expected
        if name == "PI":
            cursor.execute("SELECT %s + 0.0000000000000000", expression)
            full = cursor.fetchone()
            assert full is not None and isinstance(full[0], float)
            assert math.isclose(full[0], math.pi, rel_tol=1e-14)
