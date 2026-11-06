"""Aware input consumes statement-local UTC facts without policing other values."""

from collections.abc import Generator
from contextlib import contextmanager
from datetime import datetime, timedelta, timezone
from uuid import uuid4

import pytest
from sqlaero import Connection
from sqlaero.errors import InterfaceError, SQLAeroErrorCode
from test_public_connection import live, requires_mysql


AWARE = datetime(2024, 1, 2, 3, 4, 5, 123456,
                 tzinfo=timezone(timedelta(hours=5, minutes=30)))
UTC_VALUE = AWARE.astimezone(timezone.utc).replace(tzinfo=None)


@contextmanager
def timestamp_table(connection: Connection) -> Generator[str, None, None]:
    """Use one connection-local table and always close its setup Cursor."""
    name = "sqlaero_hint_" + uuid4().hex
    with connection.cursor() as setup:
        setup.execute(f"CREATE TEMPORARY TABLE {name}(stamp TIMESTAMP(6))")
        try:
            yield name
        finally:
            setup.execute(f"DROP TEMPORARY TABLE IF EXISTS {name}")


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("prepared", [False, True])
@pytest.mark.parametrize("variable", ["time_zone", "timé_zone"])
def test_non_utc_hint_rejects_aware_input_before_the_user_statement(
    label: str, prepared: bool, variable: str,
) -> None:
    with live(label) as connection, timestamp_table(connection) as table:
        with connection.cursor(prepared=prepared) as cursor:
            cursor.execute("SELECT 17 AS previous_result")
            sql = f"INSERT /*+ SET_VAR({variable}='+02:00') */ INTO {table} VALUES (%s)"
            with pytest.raises(InterfaceError) as caught:
                cursor.execute(sql, (AWARE,))
            assert caught.value.code == SQLAeroErrorCode.SESSION_STATE
            assert caught.value.is_server_error is False
            notes = "\n".join(caught.value.__notes__)
            assert "SET_VAR" in notes and "time_zone" in notes
            assert "SQL character" in notes
            assert cursor.fetchall() == ((17,),)
        with connection.cursor() as check:
            check.execute(f"SELECT COUNT(*) FROM {table}")
            assert check.fetchone() == (0,)
            check.execute("SELECT @@time_zone")
            assert check.fetchone() == ("+00:00",)


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("prepared", [False, True])
def test_local_preview_uses_the_same_statement_utc_precondition(
    label: str, prepared: bool,
) -> None:
    with live(label) as connection, connection.cursor(prepared=prepared) as cursor:
        cursor.execute("SELECT 23 AS previous_result")
        sql = "SELECT /*+ SET_VAR(time_zone='SYSTEM') */ %s"
        with pytest.raises(InterfaceError) as caught:
            cursor.mogrify(sql, (AWARE,))
        assert caught.value.code == SQLAeroErrorCode.SESSION_STATE
        assert caught.value.is_server_error is False
        assert "SQL character" in "\n".join(caught.value.__notes__)
        assert cursor.fetchall() == ((23,),)
        assert isinstance(cursor.mogrify(sql, (None,)), str)
        assert isinstance(connection.escape(AWARE), str)


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("prepared", [False, True])
def test_late_outer_hint_covers_earlier_cte_parameter_in_the_same_statement(
    label: str, prepared: bool,
) -> None:
    with live(label) as connection, connection.cursor(prepared=prepared) as cursor:
        sql = ("WITH v AS (SELECT %s AS dt_value) "
               "SELECT /*+ SET_VAR(time_zone='+02:00') */ UNIX_TIMESTAMP(dt_value) FROM v")
        with pytest.raises(InterfaceError) as caught:
            cursor.execute(sql, (AWARE,))
        assert caught.value.code == SQLAeroErrorCode.SESSION_STATE
        assert caught.value.is_server_error is False


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("prepared", [False, True])
def test_exact_utc_hint_preserves_the_aware_instant_and_microseconds(
    label: str, prepared: bool,
) -> None:
    with live(label) as connection, timestamp_table(connection) as table:
        with connection.cursor(prepared=prepared) as cursor:
            cursor.execute(f"INSERT /*+ SET_VAR(time_zone='+00:00') */ INTO {table} VALUES (%s)",
                           (AWARE,))
        with connection.cursor() as check:
            check.execute(f"SELECT stamp,@@time_zone FROM {table}")
            assert check.fetchone() == (UTC_VALUE, "+00:00")


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("prepared", [False, True])
def test_non_aware_values_keep_the_explicit_server_timezone_semantics(
    label: str, prepared: bool,
) -> None:
    naive = datetime(2024, 1, 2, 3, 4, 5, 123456)
    with live(label) as connection, timestamp_table(connection) as table:
        with connection.cursor(prepared=prepared) as cursor:
            cursor.execute(f"INSERT /*+ SET_VAR(time_zone='+02:00') */ INTO {table} VALUES (%s)",
                           (naive,))
        with connection.cursor() as check:
            check.execute(f"SELECT stamp,@@time_zone FROM {table}")
            assert check.fetchone() == (naive - timedelta(hours=2), "+00:00")


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("first", [False, True])
def test_a_statement_hint_does_not_change_another_statement_binding_context(
    label: str, first: bool,
) -> None:
    with live(label, multi_statements=True) as connection, timestamp_table(connection) as table:
        insert = f"INSERT INTO {table} VALUES (%s)"
        select = "SELECT /*+ SET_VAR(time_zone='+02:00') */ CAST(%s AS SIGNED)"
        sql = insert + "; " + select if first else select + "; " + insert
        args = (AWARE, 7) if first else (7, AWARE)
        with connection.cursor() as cursor:
            cursor.execute(sql, args)
            if not first:
                assert cursor.fetchall() == ((7,),)
            assert cursor.nextset() is True
            if first:
                assert cursor.fetchall() == ((7,),)
            assert cursor.nextset() is None
        with connection.cursor() as check:
            check.execute(f"SELECT stamp,@@time_zone FROM {table}")
            assert check.fetchone() == (UTC_VALUE, "+00:00")
