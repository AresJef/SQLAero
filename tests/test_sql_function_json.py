"""JSON function signatures and MySQL's actual document and value semantics."""

from collections.abc import Callable
from decimal import Decimal
from inspect import signature
from typing import Literal

import pytest
from sqlaero import Connection, CustomEscape, JSON, sqlfunc, sqltype
from sqlaero.errors import SQLAeroError
from sqlaero.typing import EscapeInput

from test_public_connection import connection as connection, requires_mysql


CASES: tuple[tuple[str, tuple[object, ...], object], ...] = (
    ("JSON_ARRAY", (1, "x"), [1, "x"]),
    ("JSON_ARRAY_APPEND", (JSON([1]), "$", 2), [1, 2]),
    ("JSON_ARRAY_INSERT", (JSON([1, 3]), "$[1]", 2), [1, 2, 3]),
    ("JSON_CONTAINS", (JSON([1, 2]), JSON(2)), 1),
    ("JSON_CONTAINS_PATH", (JSON({"a": 1}), "one", "$.a"), 1),
    ("JSON_DEPTH", (JSON({"a": [1]}),), 3),
    ("JSON_EXTRACT", (JSON({"a": "b"}), "$.a"), "b"),
    ("JSON_INSERT", (JSON({"a": 1}), "$.b", 2), {"a": 1, "b": 2}),
    ("JSON_KEYS", (JSON({"b": 1}),), ["b"]),
    ("JSON_LENGTH", (JSON([1, 2]),), 2),
    ("JSON_MERGE_PATCH", (JSON({"a": 1}), JSON({"a": 2})), {"a": 2}),
    ("JSON_MERGE_PRESERVE", (JSON({"a": 1}), JSON({"a": 2})), {"a": [1, 2]}),
    ("JSON_OBJECT", ("x", 1), {"x": 1}),
    ("JSON_OVERLAPS", (JSON([1, 2]), JSON([2, 3])), 1),
    ("JSON_PRETTY", (JSON({"a": 1}),), '{\n  "a": 1\n}'),
    ("JSON_QUOTE", ("abc",), '"abc"'),
    ("JSON_REMOVE", (JSON({"a": 1, "b": 2}), "$.b"), {"a": 1}),
    ("JSON_REPLACE", (JSON({"a": 1}), "$.a", 2), {"a": 2}),
    ("JSON_SCHEMA_VALID", (JSON({"type": "integer"}), JSON(1)), 1),
    ("JSON_SCHEMA_VALIDATION_REPORT", (JSON({"type": "integer"}), JSON(1)), {"valid": True}),
    ("JSON_SEARCH", (JSON(["abc"]), "one", "abc"), "$[0]"),
    ("JSON_SET", (JSON({"a": 1}), "$.b", 2), {"a": 1, "b": 2}),
    ("JSON_STORAGE_FREE", (JSON(1),), 0),
    ("JSON_STORAGE_SIZE", (JSON(1),), 3),
    ("JSON_TYPE", (JSON(1),), "INTEGER"),
    ("JSON_UNQUOTE", (JSON("abc"),), "abc"),
    ("JSON_VALID", (JSON({"a": 1}),), 1),
    ("JSON_VALUE", (JSON({"price": "12.3"}), "$.price"), "12.3"),
)


def test_json_function_catalog_and_special_parameter_roles() -> None:
    assert len({case[0] for case in CASES}) == 28
    for name, args, _ in CASES:
        constructor: type[object] = getattr(sqlfunc, name)
        assert name in sqlfunc.__all__
        assert constructor.__module__ == "sqlaero.sqlfunc"
        assert constructor.__base__ is sqlfunc.ABS.__base__
        assert constructor.__doc__
        signature(constructor).bind(*args)

    with pytest.raises(TypeError, match="pairs"):
        sqlfunc.JSON_OBJECT("missing value")
    with pytest.raises(TypeError, match="pairs"):
        sqlfunc.JSON_SET(JSON({}), "$.x", 1, "$.unpaired")
    with pytest.raises(ValueError, match="escape_char"):
        sqlfunc.JSON_SEARCH(JSON([]), "one", "x", "too long")
    seen: list[int] = []
    raw = CustomEscape(1, lambda value: seen.append(value) or "'x'")
    with pytest.raises(TypeError, match="escape_char"):
        sqlfunc.JSON_SEARCH(JSON([]), "one", "x", raw)  # type: ignore[arg-type]
    with pytest.raises(TypeError):
        sqlfunc.JSON_VALUE(JSON({}), raw)  # type: ignore[arg-type]
    with pytest.raises(ValueError, match="RETURNING"):
        sqlfunc.JSON_VALUE(JSON({}), "$", returning=sqltype.BINARY)
    assert not seen


@requires_mysql
@pytest.mark.parametrize(("name", "args", "expected"), CASES)
def test_json_functions_return_server_values(
    connection: Connection, name: str, args: tuple[object, ...], expected: object,
) -> None:
    constructor: Callable[..., EscapeInput] = getattr(sqlfunc, name)
    expression = constructor(*args)
    assert str(connection.escape(expression)).startswith(f"{name}(")
    with connection.cursor() as cursor:
        cursor.decode_json = True
        cursor.execute("SELECT %s", expression)
        assert cursor.fetchone() == (expected,)


@requires_mysql
def test_json_value_empty_error_and_exact_money(connection: Connection) -> None:
    document: dict[str, object] = {"price": "12.3"}
    expr = sqlfunc.JSON_VALUE(JSON(document), "$.price", returning=sqltype.DECIMAL(10, 2),
                             on_empty="null", on_error="error")
    assert "RETURNING DECIMAL(10,2) NULL ON EMPTY ERROR ON ERROR" in str(connection.escape(expr))
    with connection.cursor() as cursor:
        cursor.execute("SELECT %s", expr)
        row = cursor.fetchone()
        assert row is not None and isinstance(row[0], Decimal) and str(row[0]) == "12.30"
        del document["price"]
        cursor.execute("SELECT %s", expr)
        assert cursor.fetchone() == (None,)
        document["price"] = None
        cursor.execute("SELECT %s", expr)
        assert cursor.fetchone() == (None,)
        document["price"] = "unavailable"
        with pytest.raises(SQLAeroError) as caught:
            cursor.execute("SELECT %s", expr)
        assert caught.value.is_server_error

        policy: tuple[Literal["default"], str] = ("default", "6.75")
        fallback = sqlfunc.JSON_VALUE(JSON(document), "$.price", returning=sqltype.DECIMAL(10, 2),
                                     on_error=policy)
        cursor.execute("SELECT %s", fallback)
        assert cursor.fetchone() == (Decimal("6.75"),)
        missing = sqlfunc.JSON_VALUE(JSON({}), "$.price", on_empty=("default", "missing"),
                                     on_error="error")
        cursor.execute("SELECT %s", missing)
        assert cursor.fetchone() == ("missing",)
        for invalid in (
            sqlfunc.JSON_VALUE("invalid", "$"),
            sqlfunc.JSON_VALUE("invalid", "$", on_error=("default", "fallback")),
            sqlfunc.JSON_VALUE(JSON({}), "$..", on_error=("default", "fallback")),
            sqlfunc.JSON_VALUE(1, "$"),
        ):
            with pytest.raises(SQLAeroError) as caught:
                cursor.execute("SELECT %s", invalid)
            assert caught.value.is_server_error
