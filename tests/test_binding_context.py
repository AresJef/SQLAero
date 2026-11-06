"""R03 rejects unconfirmed text-binding contexts before sending any SQL.

These tests use ordinary values and connection-local variables only. They
verify the compiler boundary without executing an unsafe value or changing
global settings. Expected behavior comes from the finalized R03 contract.
"""

import pytest
from sqlaero import _sqlaero
from sqlaero.errors import ProgrammingError
from test_public_connection import live, requires_mysql


@pytest.mark.parametrize("prefix", [
    "SET sql_mode=''",
    "SET SESSION `sql_mode`=''",
    "SET @@SESSION.sql_mode=''",
    "SET NAMES latin1",
    "SET CHARACTER SET latin1",
    "SET character_set_connection=latin1",
    "SET collation_connection=latin1_swedish_ci",
    "SET time_zone='+01:00'",
    "SET @label=CONCAT('a', 'b'), sql_mode=''",
    "/*!80000 SET sql_mode='' */",
])
def test_text_compiler_reports_the_original_context_change(prefix: str) -> None:
    sql = prefix + "; SELECT %s"
    with pytest.raises(ProgrammingError) as caught:
        _sqlaero._bind_sql(sql, ["ordinary-value"])
    error = caught.value
    assert error.code == 65541 and error.is_server_error is False
    assert "cannot safely bind" in error.message
    assert f"SQL character {sql.index('%s')}" in error.message
    assert "execute the context change separately" in error.message


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("dictionary", [False, True])
@pytest.mark.parametrize("streaming", [False, True])
@pytest.mark.parametrize("change", ["SET sql_mode=''", "SET NAMES latin1"])
def test_public_rejection_sends_nothing_and_preserves_the_old_result(
    label: str, dictionary: bool, streaming: bool, change: str,
) -> None:
    before: tuple[object, ...] | None = None
    with live(label, multi_statements=True) as connection:
        with connection.cursor() as setup:
            setup.execute("SET @r03_sent=0")
            setup.execute("SELECT @@sql_mode, @@character_set_client")
            before = setup.fetchone()
        with connection.cursor(dict if dictionary else tuple, streaming=streaming) as cursor:
            cursor.execute("SELECT 7 AS previous_value")
            assert cursor.fetchall() == (({"previous_value": 7},) if dictionary else ((7,),))
            columns = cursor.columns
            count = cursor.rowcount
            with pytest.raises(ProgrammingError) as caught:
                cursor.execute("SET @r03_sent=1; " + change + "; SELECT %s", ["ordinary-value"])
            assert caught.value.code == 65541
            assert caught.value.is_server_error is False
            assert cursor.columns is columns
            assert cursor.rowcount == count == 1
            assert cursor.fetchall() == ()
        with connection.cursor() as after:
            after.execute("SELECT @r03_sent, @@sql_mode, @@character_set_client")
            assert before is not None
            assert after.fetchone() == (0, *before)


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
def test_confirmed_context_changes_can_be_followed_by_a_separate_bound_request(label: str) -> None:
    with live(label, multi_statements=True) as connection:
        with connection.cursor() as cursor:
            cursor.execute("SET NAMES latin1")
            cursor.execute("SELECT %s", ["é"])
            assert cursor.fetchone() == ("é",)


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
def test_native_prepare_keeps_the_server_multi_statement_error(label: str) -> None:
    with live(label, multi_statements=True) as connection:
        with connection.cursor(prepared=True) as cursor:
            with pytest.raises(ProgrammingError) as caught:
                cursor.execute("SET NAMES latin1; SELECT %s", ["ordinary-value"])
            assert caught.value.code == 1064
            assert caught.value.is_server_error is True


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
def test_mogrify_uses_the_same_local_context_precondition(label: str) -> None:
    with live(label, multi_statements=True) as connection:
        with connection.cursor() as cursor:
            with pytest.raises(ProgrammingError, match="cannot safely bind"):
                cursor.mogrify("SET sql_mode=''; SELECT %s", ["ordinary-value"])
            cursor.execute("SELECT %s; SELECT %s", [7, 8])
            assert cursor.fetchone() == (7,)
            assert cursor.nextset() is True
            assert cursor.fetchone() == (8,)
            assert cursor.nextset() is None
