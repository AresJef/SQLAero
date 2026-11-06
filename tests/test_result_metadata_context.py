"""Confirmed result metadata contexts and the explicit SS internal-change limit.

The SQL source and the first optional alias use utf8mb4. After the explicit SET,
MySQL's latin1 result bytes E9 represent U+00E9 in both the label and value;
the initial ASCII HEX/collation query independently confirms that value.
"""

import pytest
from sqlaero import Columns
from sqlaero.errors import NotSupportedError, SQLAeroErrorCode
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
    """Buffered CALL supports the change; SS diagnoses it at the real End."""
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
            cursor = connection.cursor(dict if dictionary else tuple, streaming=streaming)
            try:
                cursor.execute("CALL " + routine + "()")
                if utf8_prefix:
                    first_columns = cursor.columns
                    assert first_columns.names == ("中",)
                    assert cursor.fetchall() == expected_row("中", "utf8", dictionary)
                    assert cursor.nextset() is True

                if streaming:
                    # The Header cannot know the new results charset. Read
                    # through the actual End before requiring its clear limit;
                    # do not force early decoding of these latin1 label bytes.
                    with pytest.raises(NotSupportedError) as caught:
                        cursor.fetchall()
                    assert caught.value.code == SQLAeroErrorCode.UNSUPPORTED_CAPABILITY
                    assert caught.value.is_server_error is False
                    assert "character_set_results" in caught.value.message
                    assert "stream" in caught.value.message.lower()
                    assert cursor.row_count == 1
                    # One actual CALL request contributes only its first set.
                    assert cursor.batch_row_count == 1
                    assert connection.closed, "the rest of this CALL is still unread"
                else:
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
            finally:
                if streaming:
                    connection.close()
                else:
                    cursor.close()

            if not streaming:
                with connection.cursor() as after:
                    after.execute("SELECT @@character_set_results AS current_encoding,"
                                  "CHAR(233 USING latin1) AS `é`")
                    assert after.columns.names == ("current_encoding", "é")
                    assert after.fetchone() == ("latin1", "é")
        finally:
            with live(label, charset="utf8mb4") as cleanup_connection, cleanup_connection.cursor() as cleanup:
                cleanup.execute("DROP PROCEDURE IF EXISTS " + routine)

    if not streaming:
        assert latin_columns is not None and latin_columns.names == ("é", "wire_hex", "wire_collation")
    if utf8_prefix:
        assert first_columns is not None and first_columns.names == first_columns.keys == ("中",)


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("dictionary", [False, True])
def test_explicit_results_charset_before_call_is_supported_by_streaming(
    label: str, dictionary: bool,
) -> None:
    routine = "`sqlaero_test`.`d_metadata_before_" + uuid4().hex + "`"
    columns: Columns | None = None
    with live(label, charset="utf8mb4") as connection:
        with connection.cursor() as setup:
            setup.execute("CREATE PROCEDURE " + routine + "() BEGIN "
                          "SELECT CHAR(233 USING latin1) AS `é`; END")
        try:
            with connection.cursor() as setting:
                setting.execute("SET character_set_results=latin1")
            with connection.cursor(dict if dictionary else tuple, streaming=True) as cursor:
                assert cursor.execute("CALL " + routine + "()") is None
                columns = cursor.columns
                assert columns.names == columns.keys == ("é",)
                assert cursor.fetchall() == expected_row("é", "é", dictionary)
                assert cursor.row_count == cursor.batch_row_count == 1
                assert cursor.nextset() is True
                assert cursor.description is None and cursor.columns.names == ()
                assert cursor.nextset() is None
            assert connection.ping() is True
        finally:
            with live(label, charset="utf8mb4") as cleanup_connection, cleanup_connection.cursor() as cleanup:
                cleanup.execute("DROP PROCEDURE IF EXISTS " + routine)
    assert columns is not None and columns.names == columns.keys == ("é",)


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("dictionary", [False, True])
def test_streaming_internal_charset_change_preserves_already_delivered_ascii_row(
    label: str, dictionary: bool,
) -> None:
    routine = "`sqlaero_test`.`d_metadata_ascii_" + uuid4().hex + "`"
    columns: Columns | None = None
    first: object = None
    with live(label, charset="utf8mb4") as connection:
        with connection.cursor() as setup:
            setup.execute("CREATE PROCEDURE " + routine + "() BEGIN "
                          "SET character_set_results=latin1; "
                          "SELECT 'first' AS value UNION ALL SELECT 'second'; "
                          "SELECT 9 AS unread_tail; END")
        try:
            cursor = connection.cursor(dict if dictionary else tuple, streaming=True)
            assert cursor.execute("CALL " + routine + "()") is None
            columns = cursor.columns
            assert columns.names == columns.keys == ("value",)
            first = cursor.fetchone()
            assert first == ({"value": "first"} if dictionary else ("first",))
            assert cursor.row_count is None
            with pytest.raises(NotSupportedError) as caught:
                cursor.fetchall()
            assert caught.value.code == SQLAeroErrorCode.UNSUPPORTED_CAPABILITY
            assert caught.value.is_server_error is False
            assert "character_set_results" in caught.value.message
            assert "stream" in caught.value.message.lower()
            assert cursor.row_count == cursor.batch_row_count == 2
            assert cursor.columns is columns
            assert connection.closed, "later CALL results and its tail have not been read"
            assert first == ({"value": "first"} if dictionary else ("first",))
        finally:
            connection.close()
            with live(label, charset="utf8mb4") as cleanup_connection, cleanup_connection.cursor() as cleanup:
                cleanup.execute("DROP PROCEDURE IF EXISTS " + routine)
    assert columns is not None and columns.names == columns.keys == ("value",)
    assert first == ({"value": "first"} if dictionary else ("first",))


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("dictionary", [False, True])
def test_early_unicode_metadata_failure_does_not_claim_to_detect_internal_change(
    label: str, dictionary: bool,
) -> None:
    routine = "`sqlaero_test`.`d_metadata_early_" + uuid4().hex + "`"
    with live(label, charset="utf8mb4") as connection:
        with connection.cursor() as setup:
            setup.execute("CREATE PROCEDURE " + routine + "() BEGIN "
                          "SET character_set_results=latin1; "
                          "SELECT 'ascii' AS `é`; SELECT 9 AS unread_tail; END")
        try:
            cursor = connection.cursor(dict if dictionary else tuple, streaming=True)
            assert cursor.execute("CALL " + routine + "()") is None
            with pytest.raises(UnicodeDecodeError) as early:
                _ = cursor.columns.names
            assert early.value.encoding == "utf8mb4"
            assert early.value.object == b"\xe9"
            assert (early.value.start, early.value.end) == (0, 1)
            assert cursor.row_count is None and not connection.closed
            with pytest.raises(NotSupportedError) as actual_end:
                cursor.fetchall()
            assert actual_end.value.code == SQLAeroErrorCode.UNSUPPORTED_CAPABILITY
            assert actual_end.value.is_server_error is False
            assert "character_set_results" in actual_end.value.message
            assert "stream" in actual_end.value.message.lower()
            assert cursor.row_count == cursor.batch_row_count == 1
            assert connection.closed
        finally:
            connection.close()
            with live(label, charset="utf8mb4") as cleanup_connection, cleanup_connection.cursor() as cleanup:
                cleanup.execute("DROP PROCEDURE IF EXISTS " + routine)
