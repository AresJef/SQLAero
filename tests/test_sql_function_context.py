"""Context-bound syntax, compatibility functions, and session lock ownership."""

from collections.abc import Callable

import pytest
from sqlaero import Column, Connection, CustomEscape, JSON, sqlfunc, sqltype
from sqlaero.errors import MySQLErrorCode, ProgrammingError, SQLAeroError
from sqlaero.typing import EscapeInput

from test_public_connection import connection as connection, live, requires_mysql


def test_contextual_functions_reject_invalid_roles() -> None:
    with pytest.raises(TypeError, match="Column"):
        sqlfunc.DEFAULT("price")  # type: ignore[arg-type]
    with pytest.raises(TypeError, match="Column"):
        sqlfunc.VALUES("price")  # type: ignore[arg-type]
    with pytest.raises(TypeError, match="Column"):
        sqlfunc.MATCH("title", "bicycle")  # type: ignore[arg-type]
    with pytest.raises(ValueError, match="columns"):
        sqlfunc.MATCH([], "bicycle")
    with pytest.raises(ValueError, match="mode"):
        sqlfunc.MATCH((Column("title"),), "bicycle", mode="ORDER BY title")
    with pytest.raises(TypeError, match="literal"):
        sqlfunc.NAME_CONST("name", sqlfunc.ABS(-2))  # type: ignore[arg-type]
    with pytest.raises(ValueError, match="CHAR"):
        sqlfunc.WEIGHT_STRING("a", sqltype.CHAR())
    with pytest.raises(ValueError, match="CHAR"):
        sqlfunc.WEIGHT_STRING("a", sqltype.CHAR(3, charset="utf8mb4"))
    seen: list[int] = []
    raw = CustomEscape(1, lambda value: seen.append(value) or "'abc'")
    with pytest.raises(TypeError, match="literal"):
        sqlfunc.NAME_CONST("name", raw)  # type: ignore[arg-type]
    with pytest.raises(TypeError, match="data value"):
        sqlfunc.MATCH((Column("title"),), raw)
    assert not seen


@requires_mysql
def test_context_bound_queries(connection: Connection) -> None:
    with connection.cursor() as cursor:
        cursor.execute("CREATE TEMPORARY TABLE sqlaero_upsert (id INT PRIMARY KEY, price INT DEFAULT 5)")
        cursor.execute("INSERT INTO sqlaero_upsert (id) VALUES (1)")
        cursor.execute("SELECT %s FROM sqlaero_upsert", sqlfunc.DEFAULT(Column("price")))
        assert cursor.fetchone() == (5,)
        value = sqlfunc.VALUES(Column("price"))
        cursor.execute("INSERT INTO sqlaero_upsert VALUES (1,9) ON DUPLICATE KEY UPDATE price=%s", value)
        cursor.execute("SELECT price FROM sqlaero_upsert")
        assert cursor.fetchone() == (9,)
        expr = sqlfunc.NAME_CONST("a'b", -14)
        cursor.execute("SELECT %s", expr)
        assert cursor.fetchone() == (-14,)
        assert cursor.description is not None and cursor.description[0][0] == "a'b"

        cursor.execute("DROP TABLE IF EXISTS sqlaero_sqlfunc_ft_test")
        try:
            cursor.execute(
                "CREATE TABLE sqlaero_sqlfunc_ft_test (id INT PRIMARY KEY, title TEXT, body TEXT, "
                "FULLTEXT KEY ft(title,body)) ENGINE=InnoDB"
            )
            cursor.execute(
                "INSERT INTO sqlaero_sqlfunc_ft_test VALUES "
                "(1,'Fast bicycle','suitable for trails'), (2,'Red canoe','suitable for lakes')"
            )
            connection.commit()
            match = sqlfunc.MATCH([Column("title"), Column("body")], "+bicycle", mode="IN BOOLEAN MODE")
            assert str(connection.escape(match)) == "MATCH(`title`,`body`) AGAINST('+bicycle' IN BOOLEAN MODE)"
            cursor.execute("SELECT id FROM sqlaero_sqlfunc_ft_test WHERE %s", match)
            assert cursor.fetchall() == ((1,),)
        finally:
            cursor.execute("DROP TABLE IF EXISTS sqlaero_sqlfunc_ft_test")


@requires_mysql
def test_community_compatibility_calls(connection: Connection) -> None:
    cases: tuple[tuple[EscapeInput, str], ...] = (
        (sqlfunc.BENCHMARK(1, sqlfunc.ABS(-2)), "BENCHMARK(1,ABS(-2))"),
        (sqlfunc.FOUND_ROWS(), "FOUND_ROWS()"),
        (sqlfunc.JSON_MERGE(JSON({"a": 1}), JSON({"a": 2})), """JSON_MERGE('{"a":1}','{"a":2}')"""),
        (sqlfunc.ROLES_GRAPHML(), "ROLES_GRAPHML()"),
        (sqlfunc.SLEEP(0), "SLEEP(0)"),
        (sqlfunc.STATEMENT_DIGEST("SELECT 2"), "STATEMENT_DIGEST('SELECT 2')"),
        (sqlfunc.STATEMENT_DIGEST_TEXT("SELECT 2"), "STATEMENT_DIGEST_TEXT('SELECT 2')"),
        (sqlfunc.WEIGHT_STRING("a"), "WEIGHT_STRING('a')"),
        (sqlfunc.WEIGHT_STRING("a", sqltype.BINARY(3)), "WEIGHT_STRING('a' AS BINARY(3))"),
        (sqlfunc.WEIGHT_STRING("a", sqltype.CHAR(3)), "WEIGHT_STRING('a' AS CHAR(3))"),
        (sqlfunc.FORMAT_BYTES(1024), "FORMAT_BYTES(1024)"),
        (sqlfunc.FORMAT_PICO_TIME(1_000_000), "FORMAT_PICO_TIME(1000000)"),
        (sqlfunc.PS_CURRENT_THREAD_ID(), "PS_CURRENT_THREAD_ID()"),
        (sqlfunc.PS_THREAD_ID(sqlfunc.CONNECTION_ID()), "PS_THREAD_ID(CONNECTION_ID())"),
        (sqlfunc.ExtractValue("<a><b>A</b></a>", "/a/b"), "ExtractValue('<a><b>A</b></a>','/a/b')"),
        (sqlfunc.UpdateXML("<a><b>A</b></a>", "/a/b", "<b>B</b>"),
         "UpdateXML('<a><b>A</b></a>','/a/b','<b>B</b>')"),
    )
    with connection.cursor() as cursor:
        for expr, control in cases:
            cursor.execute(f"SELECT %s,{control}", expr)
            row = cursor.fetchone()
            assert row is not None and row[0] == row[1], (control, row)

        cursor.execute("SELECT VERSION()")
        server = cursor.fetchone()
        assert server is not None and isinstance(server[0], str)
        release = tuple(map(int, server[0].partition("-")[0].split(".")[:3]))
        assert len(release) == 3
        is_84 = release[:2] == (8, 4)
        # The four-part version was added at different patches in the two LTS series.
        minimum = (8, 4, 13) if is_84 else (9, 7, 4)
        version_with_update = sqlfunc.VERSION_WITH_UPDATE()
        assert str(connection.escape(version_with_update)) == "VERSION_WITH_UPDATE()"
        if release >= minimum:
            cursor.execute("SELECT %s,VERSION_WITH_UPDATE()", version_with_update)
            row = cursor.fetchone()
            assert row is not None and row[0] == row[1] and isinstance(row[0], str)
            assert len(row[0].partition("-")[0].split(".")) == 4
        else:
            with pytest.raises(ProgrammingError, match="VERSION_WITH_UPDATE") as caught:
                cursor.execute("SELECT %s", version_with_update)
            assert caught.value.code == MySQLErrorCode.ER_SP_DOES_NOT_EXIST and caught.value.is_server_error
        if is_84:
            with pytest.raises(SQLAeroError) as caught:
                cursor.execute("SELECT %s", sqlfunc.ETAG("a", "b"))
            assert caught.value.is_server_error
        else:
            cursor.execute("SELECT %s,ETAG('a','b')", sqlfunc.ETAG("a", "b"))
            row = cursor.fetchone()
            assert row is not None and row[0] == row[1] and isinstance(row[0], str) and len(row[0]) == 32
        for name in ("DISTANCE", "VECTOR_DISTANCE"):
            constructor: Callable[..., EscapeInput] = getattr(sqlfunc, name)
            vector = b"\x00\x00\x80\x3f\x00\x00\x00\x40"
            expr = constructor(vector, vector, "COSINE")
            assert str(connection.escape(expr)).startswith(name + "(")
            with pytest.raises(SQLAeroError) as caught:
                cursor.execute("SELECT %s", expr)
            assert caught.value.is_server_error and name in str(caught.value).upper()


@requires_mysql
def test_session_lock_switches_by_connection(connection: Connection) -> None:
    name = "sqlaero-sqlfunc-session-lock"
    with connection.cursor() as owner:
        owner.execute("SELECT VERSION(), CONNECTION_ID()")
        info = owner.fetchone()
        assert info is not None and isinstance(info[0], str)
        with live("84" if info[0].startswith("8.4") else "97") as other, other.cursor() as peer:
            try:
                owner.execute("SELECT %s", sqlfunc.IS_FREE_LOCK(name))
                assert owner.fetchone() == (1,)
                owner.execute("SELECT %s", sqlfunc.GET_LOCK(name, 0))
                assert owner.fetchone() == (1,)
                peer.execute("SELECT %s,%s,%s", (
                    sqlfunc.IS_USED_LOCK(name), sqlfunc.RELEASE_LOCK(name), sqlfunc.GET_LOCK(name, 0),
                ))
                assert peer.fetchone() == (info[1], 0, 0)
                owner.execute("SELECT %s", sqlfunc.RELEASE_LOCK(name))
                assert owner.fetchone() == (1,)
                owner.execute("SELECT %s", sqlfunc.GET_LOCK(name, 0))
                assert owner.fetchone() == (1,)
                owner.execute("SELECT %s", sqlfunc.RELEASE_ALL_LOCKS())
                assert owner.fetchone() == (1,)
                peer.execute("SELECT %s", sqlfunc.IS_FREE_LOCK(name))
                assert peer.fetchone() == (1,)
            finally:
                owner.execute("SELECT RELEASE_ALL_LOCKS()")
