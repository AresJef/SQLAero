"""Public thin-command behavior on the A lane's owned MySQL sessions."""

import pytest
import sqlaero
from sqlaero.errors import DatabaseError, InterfaceError, SQLAeroError
from test_public_connection import live, requires_mysql


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
def test_show_warnings_uses_current_diagnostics_without_changing_business_cursor(label: str) -> None:
    with live(label) as connection, connection.cursor() as business, connection.cursor() as diagnostic:
        assert getattr(connection, "show_warnings")() == ()
        business.execute("SELECT 10 AS id UNION ALL SELECT 20")
        assert business.fetchone() == (10,)
        columns, description = business.columns, business.description
        counts = (business.row_count, business.warning_count, business.lastrowid)
        diagnostic.execute("SELECT CAST('not-a-number' AS UNSIGNED)")
        result: tuple[tuple[str, int, str], ...] = getattr(connection, "show_warnings")()
        assert type(result) is tuple and len(result) == 1
        level, code, message = result[0]
        assert type(level) is str and type(code) is int and type(message) is str
        assert (level, code) == ("Warning", 1292) and "not-a-number" in message
        assert business.columns is columns and business.description == description
        assert (business.row_count, business.warning_count, business.lastrowid) == counts
        assert business.fetchone() == (20,)
        diagnostic.execute("SELECT 1")
        assert (result[0][0], result[0][1]) == ("Warning", 1292)


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
def test_show_warnings_after_error_reads_its_actual_metadata_without_a_prequery(label: str) -> None:
    connection = live(label)
    result: tuple[tuple[str, int, str], ...] = ()
    with connection.cursor() as cursor:
        with pytest.raises(DatabaseError) as failure:
            cursor.execute("SIGNAL SQLSTATE '45000' SET MYSQL_ERRNO=1644, MESSAGE_TEXT='诊断消息'")
        assert failure.value.code == 1644 and failure.value.is_server_error is True
        result = getattr(connection, "show_warnings")()
        assert result == (("Error", 1644, "诊断消息"),)
    connection.close()
    assert result == (("Error", 1644, "诊断消息"),)
    with pytest.raises(InterfaceError):
        getattr(connection, "show_warnings")()


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
def test_savepoint_api_preserves_native_transaction_and_cursor_state(label: str) -> None:
    with live(label, autocommit=False) as connection, connection.cursor() as cursor:
        cursor.execute("CREATE TEMPORARY TABLE a_savepoint_data (v INT PRIMARY KEY)")
        connection.begin()
        cursor.execute("INSERT a_savepoint_data VALUES (1)")
        name = " x`'; SET @injected=1;# "
        assert getattr(connection, "create_savepoint")(name) is None
        cursor.execute("INSERT a_savepoint_data VALUES (2)")
        assert getattr(connection, "rollback_to_savepoint")(name) is None
        cursor.execute("SELECT v FROM a_savepoint_data ORDER BY v")
        assert cursor.fetchall() == ((1,),)
        cursor.execute("INSERT a_savepoint_data VALUES (2)")
        assert getattr(connection, "create_savepoint")(name) is None
        cursor.execute("INSERT a_savepoint_data VALUES (3)")
        assert getattr(connection, "rollback_to_savepoint")(name) is None
        cursor.execute("SELECT v FROM a_savepoint_data ORDER BY v")
        assert cursor.fetchone() == (1,)
        columns, description = cursor.columns, cursor.description
        assert getattr(connection, "release_savepoint")(name) is None
        assert cursor.columns is columns and cursor.description == description
        assert cursor.fetchone() == (2,)
        with pytest.raises(DatabaseError) as failure:
            getattr(connection, "rollback_to_savepoint")(name)
        assert failure.value.code == 1305 and failure.value.is_server_error is True
        connection.commit()
        with pytest.raises(DatabaseError) as failure:
            getattr(connection, "rollback_to_savepoint")(name)
        assert failure.value.code == 1305
        cursor.execute("SELECT @injected")
        assert cursor.fetchone() == (None,)


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
def test_savepoint_success_without_a_transaction_does_not_promise_a_retained_marker(label: str) -> None:
    with live(label, autocommit=True) as connection:
        assert getattr(connection, "create_savepoint")("ephemeral") is None
        with pytest.raises(DatabaseError) as failure:
            getattr(connection, "rollback_to_savepoint")("ephemeral")
        assert failure.value.code == 1305


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
def test_init_sql_keeps_final_settings_and_active_transaction_and_discards_results(label: str) -> None:
    steps = ("SET SESSION autocommit=0, wait_timeout=321", "CREATE TEMPORARY TABLE a_init_data (v INT)",
             "INSERT a_init_data VALUES (7)", "SELECT v FROM a_init_data")
    with live(label, autocommit=True, init_sql=steps) as connection, connection.cursor() as cursor:
        assert connection.autocommit is False and connection.wait_timeout == 321
        cursor.execute("SELECT v FROM a_init_data")
        assert cursor.fetchall() == ((7,),)
        connection.rollback()
        cursor.execute("SELECT v FROM a_init_data")
        assert cursor.fetchall() == ()


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
def test_init_sql_final_utc_policy_and_connect_budget_do_not_use_operation_timeout(label: str) -> None:
    with pytest.raises(SQLAeroError, match="UTC|time_zone"):
        live(label, init_sql="SET SESSION time_zone='+08:00'")
    connection = live(label, connect_timeout=5, operation_timeout=1e-9,
                      init_sql=("SET SESSION time_zone='+08:00'", "SELECT SLEEP(0.02)",
                                "SET SESSION time_zone='+00:00'"))
    assert connection.operation_timeout == 1e-9
    connection.operation_timeout = None
    with connection.cursor() as cursor:
        cursor.execute("SELECT @@session.time_zone")
        assert cursor.fetchone() == ("+00:00",)
    connection.close()


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
def test_init_sql_error_keeps_real_success_prefix_and_stops_later_steps(label: str) -> None:
    with live(label, autocommit=True) as inspection, inspection.cursor() as cursor:
        cursor.execute("DROP TABLE IF EXISTS a_init_receipts")
        try:
            cursor.execute("CREATE TABLE a_init_receipts (v INT PRIMARY KEY)")
            with pytest.raises(DatabaseError) as failure:
                live(label, autocommit=True, init_sql=("INSERT a_init_receipts VALUES (1)",
                    "SIGNAL SQLSTATE '45000' SET MYSQL_ERRNO=1644, MESSAGE_TEXT='init primary'",
                    "INSERT a_init_receipts VALUES (2)"))
            assert failure.value.code == 1644 and failure.value.is_server_error is True
            cursor.execute("SELECT v FROM a_init_receipts ORDER BY v")
            assert cursor.fetchall() == ((1,),)
        finally:
            cursor.execute("DROP TABLE IF EXISTS a_init_receipts")


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
def test_savepoint_rollback_retains_target_and_removes_later_markers(label: str) -> None:
    with live(label, autocommit=False) as connection, connection.cursor() as cursor:
        cursor.execute("CREATE TEMPORARY TABLE a_savepoint_order (v INT)")
        connection.begin()
        cursor.execute("INSERT a_savepoint_order VALUES (1)")
        getattr(connection, "create_savepoint")("earlier")
        cursor.execute("INSERT a_savepoint_order VALUES (2)")
        cursor.execute("SAVEPOINT later")
        cursor.execute("INSERT a_savepoint_order VALUES (3)")
        getattr(connection, "rollback_to_savepoint")("earlier")
        with pytest.raises(DatabaseError) as failure:
            getattr(connection, "rollback_to_savepoint")("later")
        assert failure.value.code == 1305 and failure.value.is_server_error is True
        cursor.execute("INSERT a_savepoint_order VALUES (4)")
        getattr(connection, "rollback_to_savepoint")("earlier")
        cursor.execute("SELECT v FROM a_savepoint_order ORDER BY v")
        assert cursor.fetchall() == ((1,),)
        connection.rollback()
        with pytest.raises(DatabaseError) as failure:
            getattr(connection, "release_savepoint")("earlier")
        assert failure.value.code == 1305 and failure.value.is_server_error is True
        cursor.execute("SELECT v FROM a_savepoint_order")
        assert cursor.fetchall() == ()


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
def test_savepoint_ddl_lifetime_comes_from_mysql_and_keeps_implicit_commit_data(label: str) -> None:
    with live(label, autocommit=False) as connection, connection.cursor() as cursor:
        cursor.execute("DROP TABLE IF EXISTS a_savepoint_ddl")
        cursor.execute("CREATE TABLE a_savepoint_ddl (v INT)")
        try:
            connection.begin()
            cursor.execute("INSERT a_savepoint_ddl VALUES (1)")
            getattr(connection, "create_savepoint")("before_ddl")
            cursor.execute("CREATE TEMPORARY TABLE a_savepoint_temp (v INT)")
            assert getattr(connection, "rollback_to_savepoint")("before_ddl") is None
            cursor.execute("ALTER TABLE a_savepoint_ddl ADD COLUMN extra INT DEFAULT 0")
            for action in ("rollback_to_savepoint", "release_savepoint"):
                with pytest.raises(DatabaseError) as failure:
                    getattr(connection, action)("before_ddl")
                assert failure.value.code == 1305 and failure.value.is_server_error is True
            connection.rollback()
            cursor.execute("SELECT v,extra FROM a_savepoint_ddl")
            assert cursor.fetchall() == ((1, 0),)
        finally:
            cursor.execute("DROP TABLE IF EXISTS a_savepoint_ddl")


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
def test_session_helpers_reject_unread_stream_without_draining_it_and_reject_closed_connection(label: str) -> None:
    from sqlaero.errors import InterfaceBusyError

    connection = live(label, autocommit=False)
    actions = ("create_savepoint", "rollback_to_savepoint", "release_savepoint")
    try:
        connection.begin()
        stream: sqlaero.SSCursor = connection.cursor(sqlaero.SSCursor)
        try:
            stream.execute("SELECT 1 AS v UNION ALL SELECT 2 UNION ALL SELECT 3")
            assert stream.fetchone() == (1,)
            columns, description = stream.columns, stream.description
            for action in actions:
                with pytest.raises(InterfaceBusyError):
                    getattr(connection, action)("busy_marker")
            with pytest.raises(InterfaceBusyError):
                getattr(connection, "show_warnings")()
            assert stream.columns is columns and stream.description == description
            assert stream.fetchall() == ((2,), (3,))
        finally:
            stream.close()
        assert getattr(connection, "create_savepoint")("after_stream") is None
        assert getattr(connection, "release_savepoint")("after_stream") is None
    finally:
        connection.close()
    for action in actions:
        with pytest.raises(InterfaceError):
            getattr(connection, action)("closed_marker")
    with pytest.raises(InterfaceError):
        getattr(connection, "show_warnings")()
