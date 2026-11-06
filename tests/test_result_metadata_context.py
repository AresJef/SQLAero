"""Each result set keeps the metadata encoding confirmed at its own header.

The SQL source and the first optional alias use utf8mb4. After the explicit SET,
MySQL's latin1 result bytes E9 represent U+00E9 in both the label and value;
the initial ASCII HEX/collation query independently confirms that value.
"""

import pytest
from sqlaero import Columns
from test_public_connection import live, requires_mysql
from uuid import uuid4


def expected_row(name: str, value: str, dictionary: bool) -> object:
    """Preserve the two public row layouts with literal Unicode expectations."""
    return ({name: value},) if dictionary else ((value,),)


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("streaming", [False, True])
@pytest.mark.parametrize("dictionary", [False, True])
@pytest.mark.parametrize("utf8_prefix", [False, True])
def test_each_set_uses_its_own_confirmed_metadata_codec(
    label: str, streaming: bool, dictionary: bool, utf8_prefix: bool,
) -> None:
    first_columns: Columns | None = None
    latin_columns: Columns | None = None
    with live(label, charset="utf8mb4", multi_statements=True) as connection:
        with connection.cursor() as oracle:
            oracle.execute("SELECT HEX(CHAR(233 USING latin1)),"
                           "COLLATION(CHAR(233 USING latin1)),@@character_set_results")
            assert oracle.fetchone() == ("E9", "latin1_swedish_ci", "utf8mb4")

        sql = ("SELECT 'utf8' AS `中`; " if utf8_prefix else "") + (
            "SET character_set_results=latin1; "
            "SELECT CHAR(233 USING latin1) AS `é`"
        )
        with connection.cursor(dict if dictionary else tuple, streaming=streaming) as cursor:
            cursor.execute(sql)
            if utf8_prefix:
                first_columns = cursor.columns
                assert cursor.fetchall() == expected_row("中", "utf8", dictionary)
                assert cursor.nextset() is True

            # SET is a real no-row set. Its metadata is empty and fetching
            # from it is outside the finalized row-result contract.
            assert cursor.description is None
            assert cursor.columns.names == ()
            assert cursor.nextset() is True

            latin_columns = cursor.columns
            assert latin_columns.names == ("é",)
            assert latin_columns.keys == ("é",)
            assert latin_columns[0].name == "é"
            description = cursor.description
            assert description is not None and description[0][0] == "é"
            assert cursor.fetchall() == expected_row("é", "é", dictionary)
            assert cursor.nextset() is None

        with connection.cursor() as oracle:
            oracle.execute("SELECT @@character_set_results")
            assert oracle.fetchone() == ("latin1",)

    assert latin_columns is not None and latin_columns.names == ("é",)
    if utf8_prefix:
        assert first_columns is not None
        assert first_columns.names == first_columns.keys == ("中",)
        assert first_columns[0].name == "中"


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("streaming", [False, True])
@pytest.mark.parametrize("dictionary", [False, True])
@pytest.mark.parametrize("utf8_prefix", [False, True])
def test_direct_call_keeps_each_header_codec_and_the_confirmed_tail_context(
    label: str, streaming: bool, dictionary: bool, utf8_prefix: bool,
) -> None:
    """A routine's internal SET emits no separate public no-row result set."""
    routine = "`sqlaero_test`.`a_metadata_" + uuid4().hex + "`"
    first_columns: Columns | None = None
    latin_columns: Columns | None = None
    with live(label, charset="utf8mb4", multi_statements=True) as connection:
        with connection.cursor() as setup:
            setup.execute("SELECT HEX(CHAR(233 USING latin1)),"
                          "COLLATION(CHAR(233 USING latin1)),@@character_set_results")
            assert setup.fetchone() == ("E9", "latin1_swedish_ci", "utf8mb4")
            prefix = "SELECT 'utf8' AS `中`; " if utf8_prefix else ""
            setup.execute("CREATE PROCEDURE " + routine + "() BEGIN " + prefix +
                          "SET character_set_results=latin1; "
                          "SELECT CHAR(233 USING latin1) AS `é`,"
                          "HEX(CHAR(233 USING latin1)) AS wire_hex,"
                          "COLLATION(CHAR(233 USING latin1)) AS wire_collation; "
                          "SELECT @@character_set_results AS later_encoding; END")
        try:
            with connection.cursor(dict if dictionary else tuple, streaming=streaming) as cursor:
                cursor.execute("CALL " + routine + "()")
                if utf8_prefix:
                    first_columns = cursor.columns
                    assert first_columns.names == ("中",)
                    assert cursor.fetchall() == expected_row("中", "utf8", dictionary)
                    assert cursor.nextset() is True

                latin_columns = cursor.columns
                assert latin_columns.names == ("é", "wire_hex", "wire_collation")
                assert latin_columns.keys == ("é", "wire_hex", "wire_collation")
                assert latin_columns[0].name == "é"
                description = cursor.description
                assert description is not None and description[0][0] == "é"
                expected = ({"é": "é", "wire_hex": "E9", "wire_collation": "latin1_swedish_ci"},)
                assert cursor.fetchall() == (expected if dictionary else (("é", "E9", "latin1_swedish_ci"),))
                assert cursor.nextset() is True
                assert cursor.columns.names == ("later_encoding",)
                assert cursor.fetchall() == expected_row("later_encoding", "latin1", dictionary)
                assert cursor.nextset() is True
                assert cursor.description is None and cursor.columns.names == ()
                assert cursor.nextset() is None

            with connection.cursor() as after:
                after.execute("SELECT @@character_set_results AS current_encoding,"
                              "CHAR(233 USING latin1) AS `é`")
                assert after.columns.names == ("current_encoding", "é")
                assert after.fetchone() == ("latin1", "é")
        finally:
            with connection.cursor() as cleanup:
                cleanup.execute("DROP PROCEDURE IF EXISTS " + routine)

    assert latin_columns is not None and latin_columns.names == ("é", "wire_hex", "wire_collation")
    if utf8_prefix:
        assert first_columns is not None and first_columns.names == first_columns.keys == ("中",)
